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

use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::log_status::NOTICE_RECOVERED;
use crate::models::Msg;

use super::connect;
use super::log_types::{LogError, LogStreamEnd, LogStreamOutcome, LOG_IDLE_TIMEOUT};

/// 单批最大行数：达到即投递，防止单条消息过大（消息在通道里要整体拷贝一次）
pub const LOG_BATCH_MAX: usize = 1024;
/// 单批最长停留时间：从批次第一行起算，超时即投递（保证慢速流的实时性）
pub const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(60);

/// follow_logs — 持续读容器日志，行按批通过 tx.send(Msg::LogBatch(epoch, lines)) 推给 consumer
///
/// 返回 `LogStreamOutcome`：
/// - `end`：`Cancelled`（被取消）/ `Eof`（容器停止或 Docker 侧干净关闭）/
///   `Idle`（超过 `LOG_IDLE_TIMEOUT` 无新行）/ `Error`（Docker 侧错误）
/// - `lines`：本次连接推送出的日志行数，调用方据此决定重连时是否重放历史
/// - `notified_recovered`：本次连接是否已提示「已恢复连接」
///
/// `cancel` 由 AppState 持有 sender；换容器/清空时 drop sender，
/// 本任务会在下一次 `select!` 立即退出，避免旧容器的日志流泄漏并污染新容器。
/// `epoch` 随每批日志回传；consumer 只接受与 AppState 当前 stream epoch 相同的批，
/// 用来丢弃切换/清空瞬间已经排进 mpsc 队列的旧日志。
/// `notify_recovered` 为真时，本次连接收到首行日志即提示「已恢复连接」
/// （只有此前提示过「正在重连」时调用方才会置真）。
/// 重连与退避策略由调用方（`follow_runtime`）负责。
pub async fn follow_logs(
    id: &str,
    // 行前缀（分组合并日志为 `[容器名]`；单容器为空串 → 保持原文）
    prefix: &str,
    tx: Sender<Msg>,
    tail_lines: i64,
    epoch: u64,
    mut cancel: tokio::sync::watch::Receiver<()>,
    notify_recovered: bool,
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
            }
        }
    };
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(&tail_lines.to_string())
        .build();

    let mut stream = docker.logs(id, Some(options));
    let mut lines: u64 = 0;
    let mut notified_recovered = false;

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
                flush_batch(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered);
                return LogStreamOutcome { end: LogStreamEnd::Cancelled, lines, notified_recovered };
            }
            _ = &mut flush_sleep => {
                do_flush = true;
            }
            _ = &mut idle_sleep => {
                flush_batch(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered);
                return LogStreamOutcome { end: LogStreamEnd::Idle, lines, notified_recovered };
            }
            item = stream.next() => {
                let Some(item) = item else {
                    flush_batch(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered);
                    return LogStreamOutcome { end: LogStreamEnd::Eof, lines, notified_recovered };
                };
                let msg = match item {
                    Ok(m) => m,
                    Err(e) => {
                        flush_batch(&tx, epoch, &mut batch, &mut lines, notify_recovered, &mut notified_recovered);
                        return LogStreamOutcome {
                            end: LogStreamEnd::Error(LogError::from_bollard(&e)),
                            lines,
                            notified_recovered,
                        };
                    }
                };
                let data = match msg {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => message,
                    _ => continue,
                };
                let line = String::from_utf8_lossy(&data).trim().to_string();
                if line.is_empty() {
                    continue;
                }
                // 分组合并日志：行首带来源标记，多路日志混在一起也能分清是谁说的
                let line = if prefix.is_empty() {
                    line
                } else {
                    format!("{prefix} {line}")
                };
                if !batch_started {
                    batch_started = true;
                    // 批次首行：拉近投递定时器，保证慢速流也能及时看到这一行
                    flush_sleep.as_mut().reset(Instant::now() + LOG_FLUSH_INTERVAL);
                }
                batch.push(line);
                if batch.len() >= LOG_BATCH_MAX {
                    do_flush = true;
                }
                // 收到数据 = 连接是活的：重置空闲超时截止时间（复用同一个 Sleep）
                idle_sleep.as_mut().reset(Instant::now() + LOG_IDLE_TIMEOUT);
            }
        }
        if do_flush {
            flush_batch(
                &tx,
                epoch,
                &mut batch,
                &mut lines,
                notify_recovered,
                &mut notified_recovered,
            );
            batch_started = false;
            // 投递后把定时器推回远处，避免空批次反复触发
            flush_sleep
                .as_mut()
                .reset(Instant::now() + Duration::from_secs(3600));
        }
    }
}

/// 投递一批日志行（空批不发消息，避免无意义的惊群唤醒）
///
/// `batch` 被取走（消息要独占所有权），返回一个同容量的新缓冲供下一批复用。
fn flush_batch(
    tx: &Sender<Msg>,
    epoch: u64,
    batch: &mut Vec<String>,
    lines: &mut u64,
    notify_recovered: bool,
    notified_recovered: &mut bool,
) {
    if batch.is_empty() {
        return;
    }
    let count = batch.len() as u64;
    let payload = std::mem::replace(batch, Vec::with_capacity(LOG_BATCH_MAX));
    *lines += count;
    let _ = tx.send(Msg::LogBatch(epoch, payload));
    // 重连后的首行日志：确认连接已恢复，提示用户
    if notify_recovered && !*notified_recovered {
        *notified_recovered = true;
        let _ = tx.send(Msg::LogStreamStatus(
            epoch,
            crate::i18n::t(NOTICE_RECOVERED),
        ));
    }
}
