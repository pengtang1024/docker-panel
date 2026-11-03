//! 容器日志流：follow_logs（持续读取 + 批量行推送 + 空闲超时）
//!
//! 结束原因 / 错误分类在 `log_types.rs`，容器详情与探针在 `container_detail.rs`，
//! 重连策略在 `follow_runtime.rs`。本模块只负责「把 Docker 的日志流搬成 Msg」。
//!
//! ## 洪峰设计（性能铁律）
//!
//! 日志洪峰可达 **4 万+ 行/秒**（约 6.7 MB/s）。逐行 `tx.send` 会让 mpsc 每行
//! 唤醒一次 consumer 线程，形成惊群：实测 CPU 占用 240%+，且 UI 线程被饿死。
//! 因此本模块遵守三条硬约束：
//!
//! 1. **整批投递**：行先攒进 `batch`，达到 `LOG_BATCH_MAX` 行或距首行
//!    `LOG_FLUSH_INTERVAL` 毫秒才发一条 `Msg::LogBatch`。通道消息数下降 3 个数量级。
//! 2. **定时器复用**：空闲超时用一个常驻 `Sleep`，每行只 `reset` 截止时间，
//!    不再每行新建 `tokio::time::timeout`（新建 = 分配 + 定时器轮插拔）。
//! 3. **零拷贝是不可能的，但拷贝最小化**：每行只做一次 UTF-8 校验 + 一次堆分配。
//!
//! ## 断流回放（timestamps + since）
//!
//! `json-file` 驱动的所有跟随流都开 `timestamps=true`（用于建立时间戳游标），
//! 首次建流与 `Clear` 不带 `since`，只有断裂后的重连才带 `since` + 纳秒游标过滤。
//! 正常段每行只切首个空格剥前缀（不解析），每批投递时只解析最后一行推进游标；
//! 回放段逐行解析、先缓冲再投递最新 `REPLAY_MAX` 行。时间戳前缀在这里剥掉，
//! **日志面板与「复制日志」的内容与修复前逐字一致**。

use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::models::Msg;

use super::connect;
use super::log_batch::{flush_normal, flush_replay, format_line, LOG_BATCH_MAX};
use super::log_ts::{
    classify_line, is_caught_up, split_prefix, FilteredLine, LogCursor, ReplayBuffer,
};
use super::log_types::{LogError, LogStreamEnd, LogStreamOutcome, LOG_IDLE_TIMEOUT};

/// 单批最长停留时间：从批次第一行起算，超时即投递（保证慢速流的实时性）
pub const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(60);

/// 回放等待上限：迟迟没有追平（超长回放 / 时钟偏差）时先把当前最新一段投递出去，
/// 避免重连后日志面板长时间空白。正常情况由 `is_caught_up` 立即触发。
const REPLAY_SETTLE: Duration = Duration::from_secs(2);

/// follow_logs — 持续读容器日志，行按批通过 tx.send(Msg::LogBatch(epoch, lines)) 推给 consumer
///
/// 返回 `LogStreamOutcome`：
/// - `end`：`Cancelled`（被取消）/ `Eof`（容器停止或 Docker 侧干净关闭）/
///   `Idle`（超过 `LOG_IDLE_TIMEOUT` 无新行）/ `Error`（Docker 侧错误）
/// - `lines`：本次连接推送出的日志行数，调用方据此决定重连时是否重放历史
/// - `notified_recovered`：本次连接是否已提示「已恢复连接」
/// - `filtered` / `last_cursor`：回放过滤行数与最后时间戳（安全阀与续传用）
///
/// `cancel` 由 AppState 持有 sender；换容器/清空时 drop sender，
/// 本任务会在下一次 `select!` 立即退出，避免旧容器的日志流泄漏并污染新容器。
/// `epoch` 随每批日志回传；consumer 只接受与 AppState 当前 stream epoch 相同的批，
/// 用来丢弃切换/清空瞬间已经排进 mpsc 队列的旧日志。
/// `notify_recovered` 为真时，本次连接收到首行日志即提示「已恢复连接」
/// （只有此前提示过「正在重连」时调用方才会置真）。
/// 重连与退避策略由调用方（`follow_runtime`）负责。
/// `timestamps` 为真时整条流都带时间戳前缀（仅 `json-file` 驱动，调用方判定）；
/// `since` 只在断裂后的重连路径非空；首次建流与 `Clear` 固定 `None`。
#[allow(clippy::too_many_arguments)]
pub async fn follow_logs(
    id: &str,
    // 行前缀（分组合并日志为 `[容器名]`；单容器为空串 → 保持原文）
    prefix: &str,
    tx: Sender<Msg>,
    tail_lines: i64,
    epoch: u64,
    mut cancel: tokio::sync::watch::Receiver<()>,
    notify_recovered: bool,
    timestamps: bool,
    since: Option<LogCursor>,
) -> LogStreamOutcome {
    use bollard::container::LogOutput;
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;
    use tokio::time::Instant;

    let docker = match connect() {
        Ok(d) => d,
        Err(e) => {
            return LogStreamOutcome {
                end: LogStreamEnd::Error(LogError {
                    message: e,
                    status_code: None,
                }),
                lines: 0,
                notified_recovered: false,
                filtered: 0,
                // 连接失败也要把游标带回去，下一次重连继续从断点续读
                last_cursor: since,
            };
        }
    };
    // 这条流是否解析/剥时间戳前缀（仅 json-file 驱动；调用方在建流时判定一次）
    let timed = timestamps;
    let options = if timed {
        let base = LogsOptionsBuilder::new()
            .follow(true)
            .stdout(true)
            .stderr(true)
            .timestamps(true);
        match since {
            // 回放：since 是秒级（该秒整段重放），tail=all 取全；重叠段由纳秒游标过滤，
            // 数量上限由 ReplayBuffer（REPLAY_MAX）收敛 → 只投递最新的一千至两千行
            Some(c) => base.tail("all").since(c.since_secs()).build(),
            // 首次建流 / Clear 重建：带时间戳建立游标，但**永不带 since**（历史不复活）
            None => base.tail(&tail_lines.to_string()).build(),
        }
    } else {
        // 非 json-file 驱动（journald / none / 探测失败）：完全保持既有行为
        LogsOptionsBuilder::new()
            .follow(true)
            .stdout(true)
            .stderr(true)
            .tail(&tail_lines.to_string())
            .build()
    };

    let mut stream = docker.logs(id, Some(options));
    let mut lines: u64 = 0;
    let mut notified_recovered = false;
    let mut cursor = since;
    let mut filtered: u64 = 0;
    // 回放缓冲：历史行先攒着，追平实时后只投递最新 REPLAY_MAX 行
    let mut replaying = since.is_some();
    let mut replay = ReplayBuffer::default();
    // 正常段「最后一行的时间戳 token」：每批投递时只解析它一次（复用容量，不分配）
    let mut last_token = String::with_capacity(40);

    // —— 行批 ——
    let mut batch: Vec<String> = Vec::with_capacity(LOG_BATCH_MAX);
    // 批次是否已开始（决定要不要拉近投递定时器）
    let mut batch_started = false;

    // —— 定时器（复用，不逐行新建）——
    // 空闲超时：从最后一次收到日志起算，超时判定连接挂死
    let idle_sleep = tokio::time::sleep_until(Instant::now() + LOG_IDLE_TIMEOUT);
    tokio::pin!(idle_sleep);
    // 投递定时器：默认推到很远（不触发），攒到首行时才拉近
    let flush_sleep = tokio::time::sleep_until(Instant::now() + Duration::from_secs(3600));
    tokio::pin!(flush_sleep);

    loop {
        // 本轮是否需要投递（攒够行数 / 批次超时）
        let mut do_flush = false;
        tokio::select! {
            biased;
            _ = cancel.changed() => {
                flush_normal(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered, &mut cursor, &mut last_token);
                flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                return LogStreamOutcome {
                    end: LogStreamEnd::Cancelled,
                    lines,
                    notified_recovered,
                    filtered,
                    last_cursor: cursor,
                };
            }
            _ = &mut flush_sleep => {
                if replaying {
                    // 回放迟迟没追平：先投递当前最新一段，转入正常接续
                    flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                    replaying = false;
                    flush_sleep.as_mut().reset(Instant::now() + Duration::from_secs(3600));
                } else {
                    do_flush = true;
                }
            }
            _ = &mut idle_sleep => {
                flush_normal(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered, &mut cursor, &mut last_token);
                flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                return LogStreamOutcome {
                    end: LogStreamEnd::Idle,
                    lines,
                    notified_recovered,
                    filtered,
                    last_cursor: cursor,
                };
            }
            item = stream.next() => {
                let Some(item) = item else {
                    flush_normal(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered, &mut cursor, &mut last_token);
                    flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                    return LogStreamOutcome {
                        end: LogStreamEnd::Eof,
                        lines,
                        notified_recovered,
                        filtered,
                        last_cursor: cursor,
                    };
                };
                let msg = match item {
                    Ok(m) => m,
                    Err(e) => {
                        flush_normal(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered, &mut cursor, &mut last_token);
                        flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                        return LogStreamOutcome {
                            end: LogStreamEnd::Error(LogError::from_bollard(&e)),
                            lines,
                            notified_recovered,
                            filtered,
                            last_cursor: cursor,
                        };
                    }
                };
                let data = match msg {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => message,
                    _ => continue,
                };
                // 顺序固定：时间戳模式先切首个空格剥前缀，再 trim，最后投递前加 [容器名]
                let raw = String::from_utf8_lossy(&data);
                let raw = raw.trim();
                let (text, ts) = if timed {
                    if replaying {
                        // 回放段：逐行解析，用纳秒游标丢弃 since 秒级重放的重叠段
                        match classify_line(raw, cursor) {
                            FilteredLine::Duplicate => {
                                filtered += 1;
                                (None, None)
                            }
                            FilteredLine::Keep { text, ts } => (Some(text.trim()), Some(ts)),
                            // 解析失败保守保留正文（宁可多一行，也不丢数据）
                            FilteredLine::Raw { text } => (Some(text.trim()), None),
                        }
                    } else {
                        // 正常段：只切首个空格剥前缀、不解析；游标留到投递时解析最后一行
                        let (rest, token) = split_prefix(raw);
                        if let Some(t) = token {
                            last_token.clear();
                            last_token.push_str(t);
                        }
                        (Some(rest.trim()), None)
                    }
                } else {
                    (Some(raw), None)
                };
                if let Some(ts) = ts {
                    cursor = Some(ts);
                }
                if let Some(text) = text {
                    if !text.is_empty() {
                        if replaying {
                            let was_empty = replay.is_empty();
                            replay.push(text.to_string());
                            if was_empty {
                                // 首行历史：2s 后仍未追平就先投递（不 reset，避免被洪峰无限推迟）
                                flush_sleep.as_mut().reset(Instant::now() + REPLAY_SETTLE);
                            }
                            let caught_up = match ts {
                                Some(t) => is_caught_up(t, now_secs()),
                                None => false,
                            };
                            if caught_up {
                                flush_replay(prefix, &tx, epoch, &mut replay, &mut lines, notify_recovered, &mut notified_recovered);
                                replaying = false;
                                flush_sleep.as_mut().reset(Instant::now() + Duration::from_secs(3600));
                            }
                        } else {
                            if !batch_started {
                                batch_started = true;
                                // 批次首行：拉近投递定时器，保证慢速流也能及时看到这一行
                                flush_sleep.as_mut().reset(Instant::now() + LOG_FLUSH_INTERVAL);
                            }
                            batch.push(format_line(prefix, text));
                            if batch.len() >= LOG_BATCH_MAX {
                                do_flush = true;
                            }
                        }
                    }
                }
                // 收到数据 = 连接是活的：重置空闲超时截止时间（复用同一个 Sleep）
                idle_sleep.as_mut().reset(Instant::now() + LOG_IDLE_TIMEOUT);
            }
        }
        if do_flush {
            flush_normal(
                &tx,
                epoch,
                &mut batch,
                &mut lines,
                notify_recovered,
                &mut notified_recovered,
                &mut cursor,
                &mut last_token,
            );
            batch_started = false;
            // 投递后把定时器推回远处，避免空批次反复触发
            flush_sleep
                .as_mut()
                .reset(Instant::now() + Duration::from_secs(3600));
        }
    }
}

/// 客户端墙钟秒数（仅用于「回放是否追平实时」判定；同机守护进程时间戳同源）
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
