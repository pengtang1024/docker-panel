//! 日志批投递：行前缀拼接 / 正常段与回放段的批量投递 / 洪峰抽样
//!
//! 从 `logs.rs` 拆出（R2：正文 ≤300 行）。这里只做「攒好的行如何发出去」，
//! 不含读流逻辑：
//! - 正常段 `flush_normal`：每批只解析**最后一行**的时间戳 token 推进游标，
//!   再把行发出去（成本优化，逐行不解析）。
//! - 回放段 `flush_replay`：把回放缓冲按时间顺序投递（单批仍不超过 `LOG_BATCH_MAX`）。
//! - `flush_batch`：唯一真正 `tx.send(Msg::LogBatch)` 的地方，空批不发（避免惊群）。
//! - `Sampling`：10 万行/秒量级的泄压阀。通道是无界队列，UI 侧跟不上时
//!   积压会无限涨（RSS / 延迟双失控），这里按积压比例丢中间行并提示用户。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::log_status::{NOTICE_RECOVERED, NOTICE_SAMPLED};
use crate::models::Msg;

use super::log_ts::{parse_ts_token, LogCursor, ReplayBuffer};

/// 单批最大行数：达到即投递，防止单条消息过大（消息在通道里要整体拷贝一次）
pub const LOG_BATCH_MAX: usize = 1024;

/// 通道积压阈值：超过它说明 UI 侧已经跟不上生产速度（约 2s 的满额消费量）
const LOG_BACKLOG_HIGH: u64 = 20_000;

/// 抽样步长上限：最坏情况每 64 行保留 1 行（10 万行/秒 → 1.5k 行/秒）
const LOG_SAMPLE_MAX: u32 = 64;

/// 抽样提示的最小刷新间隔（避免每批都发一条状态消息）
const SAMPLE_NOTICE_INTERVAL: Duration = Duration::from_secs(2);

/// 一次日志流的投递出口：帧号、通道、积压计账与「已恢复」提示状态。
///
/// 打包成结构体是为了让 `flush_*` 的参数表保持在可读范围（原本已有 7 个参数）。
pub struct LogSink<'a> {
    pub tx: &'a Sender<Msg>,
    pub epoch: u64,
    /// 已投递但尚未被 consumer 应用的行数（生产者据此感知 UI 侧跟不上）
    pub backlog: &'a Arc<AtomicU64>,
    pub notify_recovered: bool,
    pub notified_recovered: &'a mut bool,
    pub lines: &'a mut u64,
}

impl LogSink<'_> {
    /// 状态提示出口：按帧号投递一行工具条文案
    pub fn tell(&self, text: String) {
        let _ = self.tx.send(Msg::LogStreamStatus(self.epoch, text));
    }
}

/// 洪峰抽样状态（每条流独立）
///
/// 策略是**反馈式**的：步长 = 积压 / 阈值，积压越高丢得越多，生产速率随之回落，
/// 于是积压自己收敛在阈值附近——不需要知道 consumer 的确切消费能力。
/// 游标不受影响：被丢掉的行仍然参与了「最后一行时间戳」的更新（见 `logs.rs`）。
pub struct Sampling {
    stride: u32,
    seq: u32,
    dropped: u64,
    last_notice: Option<Instant>,
    noticed_dropped: u64,
}

impl Sampling {
    pub fn new() -> Self {
        Self {
            stride: 1,
            seq: 0,
            dropped: 0,
            last_notice: None,
            noticed_dropped: 0,
        }
    }

    /// 按当前积压更新步长；返回步长是否发生变化
    pub fn refresh(&mut self, backlog: u64) -> bool {
        let next = ((backlog / LOG_BACKLOG_HIGH) as u32).clamp(1, LOG_SAMPLE_MAX);
        if next == self.stride {
            return false;
        }
        self.stride = next;
        self.seq = 0;
        // 状态迁移留痕：抽样只在工具条上提示，stderr 这份是排障/核对用的
        if next > 1 {
            eprintln!("[logs] flood sampling 1/{next} (backlog {backlog} lines)");
        } else {
            eprintln!("[logs] flood sampling off (backlog {backlog} lines)");
        }
        true
    }

    /// 该行是否保留（丢弃时累计计数，供提示用）
    pub fn keep(&mut self) -> bool {
        if self.stride == 1 {
            return true;
        }
        self.seq = self.seq.wrapping_add(1);
        if self.seq.is_multiple_of(self.stride) {
            true
        } else {
            self.dropped += 1;
            false
        }
    }

    /// 抽样状态提示（只有单流才调用：多流合并时状态栏文案会互相覆盖）。
    ///
    /// 进入抽样 / 丢弃数变化时提示；退出抽样时清空提示。最多每 2s 一条。
    pub fn notice(&mut self, sink: &LogSink) {
        if self.stride > 1 {
            let due = self
                .last_notice
                .is_none_or(|t| t.elapsed() >= SAMPLE_NOTICE_INTERVAL);
            if self.dropped != self.noticed_dropped && due {
                self.last_notice = Some(Instant::now());
                self.noticed_dropped = self.dropped;
                sink.tell(crate::i18n::trf(
                    NOTICE_SAMPLED,
                    &[&self.dropped.to_string()],
                ));
            }
        } else if self.last_notice.is_some() {
            // 积压回落：清掉工具条上的洪峰提示（空串即无提示）
            self.last_notice = None;
            self.noticed_dropped = self.dropped;
            sink.tell(String::new());
        }
    }
}

/// 行前缀 + 正文（单容器前缀为空 → 保持原文，与修复前逐字一致）
pub fn format_line(prefix: &str, text: &str) -> String {
    if prefix.is_empty() {
        text.to_string()
    } else {
        format!("{prefix} {text}")
    }
}

/// 投递回放缓冲（按时间顺序），单条 LogBatch 仍不超过 `LOG_BATCH_MAX` 行
///
/// 回放段不抽样：`ReplayBuffer` 已经收敛到 `REPLAY_MAX` 行，再抽样只会让断流期间
/// 的日志更难看清（且不影响积压——它本来就是有界的）。
pub fn flush_replay(prefix: &str, sink: &mut LogSink, replay: &mut ReplayBuffer) {
    if replay.is_empty() {
        return;
    }
    let mut batch = Vec::with_capacity(LOG_BATCH_MAX);
    for text in replay.take() {
        batch.push(format_line(prefix, &text));
        if batch.len() >= LOG_BATCH_MAX {
            flush_batch(sink, &mut batch);
        }
    }
    flush_batch(sink, &mut batch);
}

/// 正常段投递：先解析「最后一行的时间戳 token」推进游标（每批只解析一次），
/// 再走通用投递。空批不解析（token 留到下一批，仍是本流最后一行的时间戳）。
pub fn flush_normal(
    sink: &mut LogSink,
    batch: &mut Vec<String>,
    cursor: &mut Option<LogCursor>,
    last_token: &mut String,
) {
    if batch.is_empty() {
        return;
    }
    if let Some(ts) = parse_ts_token(last_token) {
        *cursor = Some(ts);
    }
    last_token.clear();
    flush_batch(sink, batch);
}

/// 投递一批日志行（空批不发消息，避免无意义的惊群唤醒）
///
/// `batch` 被取走（消息要独占所有权），返回一个同容量的新缓冲供下一批复用。
fn flush_batch(sink: &mut LogSink, batch: &mut Vec<String>) {
    if batch.is_empty() {
        return;
    }
    let count = batch.len() as u64;
    let payload = std::mem::replace(batch, Vec::with_capacity(LOG_BATCH_MAX));
    *sink.lines += count;
    sink.backlog.fetch_add(count, Ordering::Relaxed);
    let _ = sink.tx.send(Msg::LogBatch(sink.epoch, payload));
    // 重连后的首行日志：确认连接已恢复，提示用户
    if sink.notify_recovered && !*sink.notified_recovered {
        *sink.notified_recovered = true;
        sink.tell(crate::i18n::t(NOTICE_RECOVERED));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sampling_below_threshold() {
        let mut s = Sampling::new();
        assert!(!s.refresh(LOG_BACKLOG_HIGH - 1));
        // 一行都不丢
        assert_eq!((0..1000).filter(|_| s.keep()).count(), 1000);
        assert_eq!(s.dropped, 0);
    }

    #[test]
    fn stride_follows_backlog() {
        let mut s = Sampling::new();
        assert!(s.refresh(LOG_BACKLOG_HIGH * 4));
        // 每 4 行保留 1 行
        let kept = (0..400).filter(|_| s.keep()).count();
        assert_eq!(kept, 100);
        assert_eq!(s.dropped, 300);
        // 积压回落 → 退出抽样
        assert!(s.refresh(0));
        assert!(s.keep());
    }

    #[test]
    fn stride_is_capped() {
        let mut s = Sampling::new();
        s.refresh(u64::MAX / 2);
        // 封顶 64：只保留 1/64
        let kept = (0..6400).filter(|_| s.keep()).count();
        assert_eq!(kept, 100);
    }
}
