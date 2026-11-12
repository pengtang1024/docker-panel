//! 日志批投递：行前缀拼接 / 正常段与回放段的批量投递
//!
//! 从 `logs.rs` 拆出（R2：正文 ≤300 行）。这里只做「攒好的行如何发出去」，
//! 不含读流逻辑：
//! - 正常段 `flush_normal`：每批只解析**最后一行**的时间戳 token 推进游标，
//!   再把行发出去（成本优化，逐行不解析）。
//! - 回放段 `flush_replay`：把回放缓冲按时间顺序投递（单批仍不超过 `LOG_BATCH_MAX`）。
//! - `flush_batch`：唯一真正 `tx.send(Msg::LogBatch)` 的地方，空批不发（避免惊群）。

use std::sync::mpsc::Sender;

use crate::log_status::NOTICE_RECOVERED;
use crate::models::Msg;

use super::log_ts::{parse_ts_token, LogCursor, ReplayBuffer};

/// 单批最大行数：达到即投递，防止单条消息过大（消息在通道里要整体拷贝一次）
pub const LOG_BATCH_MAX: usize = 1024;

/// 行前缀 + 正文（单容器前缀为空 → 保持原文，与修复前逐字一致）
pub fn format_line(prefix: &str, text: &str) -> String {
    if prefix.is_empty() {
        text.to_string()
    } else {
        format!("{prefix} {text}")
    }
}

/// 投递回放缓冲（按时间顺序），单条 LogBatch 仍不超过 `LOG_BATCH_MAX` 行
pub fn flush_replay(
    prefix: &str,
    tx: &Sender<Msg>,
    epoch: u64,
    replay: &mut ReplayBuffer,
    lines: &mut u64,
    notify_recovered: bool,
    notified_recovered: &mut bool,
) {
    if replay.is_empty() {
        return;
    }
    let mut batch = Vec::with_capacity(LOG_BATCH_MAX);
    for text in replay.take() {
        batch.push(format_line(prefix, &text));
        if batch.len() >= LOG_BATCH_MAX {
            flush_batch(
                tx,
                epoch,
                &mut batch,
                lines,
                notify_recovered,
                notified_recovered,
            );
        }
    }
    flush_batch(
        tx,
        epoch,
        &mut batch,
        lines,
        notify_recovered,
        notified_recovered,
    );
}

/// 正常段投递：先解析「最后一行的时间戳 token」推进游标（每批只解析一次），
/// 再走通用投递。空批不解析（token 留到下一批，仍是本流最后一行的时间戳）。
#[allow(clippy::too_many_arguments)]
pub fn flush_normal(
    tx: &Sender<Msg>,
    epoch: u64,
    batch: &mut Vec<String>,
    lines: &mut u64,
    notify_recovered: bool,
    notified_recovered: &mut bool,
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
    flush_batch(
        tx,
        epoch,
        batch,
        lines,
        notify_recovered,
        notified_recovered,
    );
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
