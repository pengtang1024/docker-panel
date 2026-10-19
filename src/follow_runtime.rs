//! 日志流运行时：建流 / 退避重连 / 存活标志
//!
//! 从 `AppState` 拆出（ui_state.rs 正文 ≤300 行约束，R2）：
//! AppState 只保留 `follow_id` / `follow_cancel` / `follow_active` / `follow_running`
//! 四个状态字段与薄封装，异步重连运行时全部收敛在本模块。
//!
//! 结局判定（`LogStreamEnd` → `FollowAction`）：
//! - `Cancelled`：切容器 / Clear 取消 → 结束
//! - `Eof`：日志流自然结束；容器已停止 → 结束；容器仍在运行 → 可恢复，按退避重连
//! - `Idle`：超过 `LOG_IDLE_TIMEOUT` 无新日志（连接可能挂死）→ 可恢复，静默重连
//! - `Error`：Docker 侧错误；致命错误（404 / 容器已删除）→ 结束，否则退避重连

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::docker::{
    follow_logs, retry_delay, LogError, LogStreamEnd, LogStreamOutcome, LOG_IDLE_TIMEOUT,
};
use crate::models::Msg;

/// 重连最大尝试次数：超过后结束流，交回 `ensure_follow` 重新决策
/// （1+2+4+8+10*4 ≈ 55s 后放弃，避免僵尸任务无限重试）
pub const MAX_RETRY_ATTEMPTS: u32 = 8;

/// 首次失败提示（i18n key）
const NOTICE_INTERRUPTED: &str = "Log stream interrupted, reconnecting…";
/// 重连提示（i18n key）
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

/// 流结束后的处置
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowAction {
    /// 结束流（取消 / 容器停止 / 致命错误）
    End,
    /// 可恢复：退避后重连；`silent` 表示无需打扰用户（如空闲超时）
    Recover { reason: String, silent: bool },
}

/// 续流 tail：历史未加载时拉最近 200 行；已加载过则只接续新行，避免 Clear 后历史复活。
pub fn follow_tail(history_done: bool) -> i64 {
    if history_done {
        0
    } else {
        200
    }
}

/// 结局分类：决定是结束还是退避重连
pub fn classify_end(end: &LogStreamEnd, container_running: bool) -> FollowAction {
    match end {
        LogStreamEnd::Cancelled => FollowAction::End,
        LogStreamEnd::Eof => {
            if container_running {
                // Docker 侧干净关闭：运行中的容器不应静默停摆
                FollowAction::Recover {
                    reason: "log stream closed unexpectedly".into(),
                    silent: false,
                }
            } else {
                FollowAction::End
            }
        }
        LogStreamEnd::Idle => FollowAction::Recover {
            reason: format!("log stream idle timeout ({}s)", LOG_IDLE_TIMEOUT.as_secs()),
            silent: true,
        },
        LogStreamEnd::Error(e) => {
            let err: &LogError = e;
            if err.is_fatal() {
                FollowAction::End
            } else {
                FollowAction::Recover {
                    reason: err.message.clone(),
                    silent: false,
                }
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
        // 是否已经向用户提示过「正在重连」
        let mut notified = false;

        loop {
            let outcome: LogStreamOutcome =
                follow_logs(&id, tx.clone(), retry_tail, epoch, cancel_rx.clone()).await;
            sent_any |= outcome.lines > 0;

            match classify_end(&outcome.end, running.load(Ordering::Acquire)) {
                FollowAction::End => {
                    if notified {
                        let _ =
                            tx.send(Msg::LogStreamStatus(epoch, crate::i18n::t(NOTICE_STOPPED)));
                    }
                    break;
                }
                FollowAction::Recover { reason, silent } => {
                    if attempt >= MAX_RETRY_ATTEMPTS {
                        eprintln!("[log-stream] {id} giving up after {attempt} retries: {reason}");
                        if !silent {
                            let _ = tx
                                .send(Msg::LogStreamStatus(epoch, crate::i18n::t(NOTICE_STOPPED)));
                        }
                        break;
                    }

                    let delay = retry_delay(attempt);
                    attempt += 1;
                    // 已推送过日志则只接续新行，避免重放历史造成重复
                    if sent_any {
                        retry_tail = 0;
                    }
                    if !silent {
                        if !notified {
                            notified = true;
                            let _ = tx.send(Msg::LogStreamStatus(
                                epoch,
                                crate::i18n::t(NOTICE_INTERRUPTED),
                            ));
                        } else if attempt.is_multiple_of(4) {
                            // 降频打印，避免 stderr 每 10s 刷屏
                            eprintln!(
                                "[log-stream] {id} still failing: {reason}; next retry in {delay:?}"
                            );
                        }
                    }

                    // 退避等待可被取消：切容器/Clear 时立即退出
                    tokio::select! {
                        _ = cancel_rx.changed() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }

                    // 即将重连：先恢复状态提示（若本次仍失败会再次提示「正在重连」）
                    if notified {
                        let _ = tx.send(Msg::LogStreamStatus(
                            epoch,
                            crate::i18n::t(NOTICE_RECOVERED),
                        ));
                        notified = false;
                    }
                }
            }
        }

        active_task.store(false, Ordering::Release);
        // 流结束通知 consumer；旧 epoch 的结束消息会被忽略
        let _ = tx.send(Msg::LogStreamEnded(epoch));
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
    fn follow_tail_avoids_replaying_history() {
        assert_eq!(follow_tail(false), 200);
        assert_eq!(follow_tail(true), 0);
    }

    #[test]
    fn classify_end_marks_recoverable_cases() {
        // 取消 → 结束
        assert_eq!(
            classify_end(&LogStreamEnd::Cancelled, true),
            FollowAction::End
        );
        // 容器停止的 EOF → 结束
        assert_eq!(classify_end(&LogStreamEnd::Eof, false), FollowAction::End);
        // 运行中却 EOF → 可恢复（自愈）
        assert!(matches!(
            classify_end(&LogStreamEnd::Eof, true),
            FollowAction::Recover { silent: false, .. }
        ));
        // 空闲超时 → 可恢复但静默
        assert!(matches!(
            classify_end(&LogStreamEnd::Idle, true),
            FollowAction::Recover { silent: true, .. }
        ));
        // 致命错误（容器已删除）→ 结束；可恢复错误 → 重连
        let fatal = crate::docker::LogError {
            message: "No such container".into(),
            status_code: Some(404),
        };
        assert_eq!(
            classify_end(&LogStreamEnd::Error(fatal), true),
            FollowAction::End
        );
        let refused = crate::docker::LogError {
            message: "Connection refused".into(),
            status_code: None,
        };
        assert!(matches!(
            classify_end(&LogStreamEnd::Error(refused), true),
            FollowAction::Recover { silent: false, .. }
        ));
    }
}
