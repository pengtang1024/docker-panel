//! 日志流运行时：建流 / 退避重连 / 存活标志
//!
//! 从 `AppState` 拆出（ui_state.rs 正文 ≤300 行约束，R2）：
//! AppState 只保留 `follow_id` / `follow_cancel` / `follow_active` / `follow_running`
//! 四个状态字段与薄封装，异步重连运行时全部收敛在本模块。
//!
//! 结局判定（`LogStreamEnd`）：
//! - `Cancelled`：切容器 / Clear 取消 → 立即结束，不重连
//! - `Eof`：日志流自然结束；容器已停止 → 结束；容器仍在运行 → 视为可恢复，按退避重连
//! - `Error`：Docker 侧错误；不可恢复（如容器不存在）→ 结束，否则退避重连

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::docker::{follow_logs, retry_delay, LogStreamEnd};
use crate::models::Msg;

/// 重连最大尝试次数：超过后结束流，交回 `ensure_follow` 重新决策
/// （1+2+4+8+10*4 ≈ 55s 后放弃，避免僵尸任务无限重试）
pub const MAX_RETRY_ATTEMPTS: u32 = 8;

/// 首次失败提示（i18n key）
const NOTICE_INTERRUPTED: &str = "Log stream interrupted, reconnecting…";
/// 重连成功提示（i18n key）
const NOTICE_RECOVERED: &str = "Log stream reconnected";
/// 放弃重连提示（i18n key）
const NOTICE_STOPPED: &str = "Log stream stopped";

/// 一条日志流的运行时句柄
pub struct FollowHandle {
    /// 取消信号：drop 即取消该流
    pub cancel: tokio::sync::watch::Sender<()>,
    /// 存活标志：该流退出时置 false（每流独立，旧流不影响新流）
    pub active: Arc<AtomicBool>,
}

/// 续流 tail：历史未加载时拉最近 200 行；已加载过则只接续新行，避免 Clear 后历史复活。
pub fn follow_tail(history_done: bool) -> i64 {
    if history_done {
        0
    } else {
        200
    }
}

/// 不可恢复错误判定：容器被删除等，重试无意义
pub fn is_unrecoverable(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("no such container") || e.contains("404") || e.contains("not found")
}

/// 结局分类：返回 Some(原因) 表示可恢复（需要退避重连），None 表示结束。
///
/// - `Cancelled`：取消 → 结束
/// - `Eof` + 容器已停止 → 结束；`Eof` + 容器仍在运行 → 可恢复（Docker 侧干净关闭）
/// - `Error` + 不可恢复 → 结束；否则可恢复
pub fn classify_end(end: &LogStreamEnd, container_running: bool) -> Option<String> {
    match end {
        LogStreamEnd::Cancelled => None,
        LogStreamEnd::Eof => {
            if container_running {
                Some("log stream closed unexpectedly".to_string())
            } else {
                None
            }
        }
        LogStreamEnd::Error(e) => {
            if is_unrecoverable(e) {
                None
            } else {
                Some(e.clone())
            }
        }
    }
}

/// 启动日志流运行时。
///
/// `running` 反映被跟随容器是否仍在运行（由 consumer 随列表刷新更新）：
/// 容器仍运行却收到 EOF 时按可恢复处理并退避重连。
pub fn spawn_follow(
    rt: &Runtime,
    id: String,
    tail: i64,
    epoch: u64,
    running: Arc<AtomicBool>,
    tx: Sender<Msg>,
    tx_end: Sender<Msg>,
) -> FollowHandle {
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(());
    let active = Arc::new(AtomicBool::new(true));
    let active_task = active.clone();
    let handle = rt.handle().clone();

    handle.spawn(async move {
        let mut attempt: u32 = 0;
        // 本流是否已推送过日志：决定重连时是否重放历史（避免重复行）
        let mut sent_any = false;
        let mut retry_tail = tail;
        let mut notified = false;

        loop {
            let (end, lines) =
                follow_logs(&id, tx.clone(), retry_tail, epoch, cancel_rx.clone()).await;
            sent_any |= lines > 0;

            // 不可恢复错误（容器被删除等）：直接结束并提示，不再僵尸式重试
            if let LogStreamEnd::Error(e) = &end {
                if is_unrecoverable(e) {
                    eprintln!("[log-stream] {id} stream ended: {e}");
                    let _ = tx.send(Msg::LogStreamStatus(crate::i18n::t(NOTICE_STOPPED)));
                    break;
                }
            }

            // 可恢复原因：None 表示本轮结束（取消 / 容器停止 / 不可恢复错误）
            let reason = classify_end(&end, running.load(Ordering::Acquire));

            let Some(reason) = reason else {
                // 结束：曾失败过则提示已恢复（容器停止/取消时不再刷状态）
                if notified && attempt > 0 {
                    let _ = tx.send(Msg::LogStreamStatus(crate::i18n::t(NOTICE_RECOVERED)));
                }
                break;
            };

            if attempt >= MAX_RETRY_ATTEMPTS {
                eprintln!("[log-stream] {id} giving up after {attempt} retries: {reason}");
                let _ = tx.send(Msg::LogStreamStatus(crate::i18n::t(NOTICE_STOPPED)));
                break;
            }

            let delay = retry_delay(attempt);
            attempt += 1;
            // 已推送过日志则只接续新行，避免重放历史造成重复
            if sent_any {
                retry_tail = 0;
            }
            if !notified {
                notified = true;
                let _ = tx.send(Msg::LogStreamStatus(crate::i18n::t(NOTICE_INTERRUPTED)));
            } else if attempt.is_multiple_of(4) {
                // 降频打印，避免 stderr 每 10s 刷屏
                eprintln!("[log-stream] {id} still failing: {reason}; next retry in {delay:?}");
            }

            // 退避等待可被取消：切容器/Clear 时立即退出
            tokio::select! {
                _ = cancel_rx.changed() => break,
                _ = tokio::time::sleep(delay) => {}
            }
        }

        active_task.store(false, Ordering::Release);
        // 流结束通知 consumer；旧 epoch 的结束消息会被忽略
        let _ = tx_end.send(Msg::LogStreamEnded(epoch));
    });

    FollowHandle {
        cancel: cancel_tx,
        active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = assert!(MAX_RETRY_ATTEMPTS > 0 && MAX_RETRY_ATTEMPTS <= 16);

    #[test]
    fn unrecoverable_errors_are_detected() {
        assert!(is_unrecoverable("No such container: abc"));
        assert!(is_unrecoverable("HTTP 404 Not Found"));
        assert!(!is_unrecoverable("connection refused"));
        assert!(!is_unrecoverable(
            "error during connect: Connection reset by peer"
        ));
    }

    #[test]
    fn follow_tail_avoids_replaying_history() {
        assert_eq!(follow_tail(false), 200);
        assert_eq!(follow_tail(true), 0);
    }

    #[test]
    fn classify_end_marks_recoverable_cases() {
        // 取消 → 结束
        assert!(classify_end(&LogStreamEnd::Cancelled, true).is_none());
        // 容器停止的 EOF → 结束
        assert!(classify_end(&LogStreamEnd::Eof, false).is_none());
        // 运行中却 EOF → 可恢复（自愈）
        assert!(classify_end(&LogStreamEnd::Eof, true).is_some());
        // 不可恢复错误 → 结束；可恢复错误 → 重连
        assert!(classify_end(&LogStreamEnd::Error("No such container".into()), true).is_none());
        assert!(classify_end(&LogStreamEnd::Error("connection refused".into()), true).is_some());
    }
}
