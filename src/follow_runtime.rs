//! 日志流运行时：建流 / 退避重连 / 存活性探针
//!
//! 从 `AppState` 拆出（ui_state.rs 正文 ≤300 行约束，R2）：
//! AppState 只保留 `follow_id` / `follow_cancel` / `follow_active` 三个状态字段
//! 与薄封装，异步重连运行时全部收敛在本模块。
//!
//! 结局判定（`LogStreamEnd` + 探针结论 → `FollowAction`）：
//! - `Cancelled`：切容器 / Clear 取消 → 结束（不打扰用户）
//! - `Eof`：日志流自然结束；探针判定容器已停止 / 已删除 → 结束；
//!   容器仍在运行 → 可恢复，按退避重连
//! - `Idle`：超过 `LOG_IDLE_TIMEOUT` 无新日志（连接可能挂死）→ 保活：
//!   静默重连，**不计入失败预算**（这是长连接健康检查，不是失败）
//! - `Error`：Docker 侧错误；致命错误（404 / 容器已删除）→ 结束，否则退避重连
//!
//! 三条铁律：
//! 1. 容器是否还活着由**探针**判定（`probe_container`），不再用跨流共享的
//!    running 标志（旧流退出时会把新流的状态写脏）。
//! 2. 连续失败预算耗尽后转 30s 低频保活，**永不放弃跟随**（旧实现会静默停止）。
//! 3. 「已恢复连接」有且只有三个触发点：重连后收到首行日志 / 空闲超时结局
//!    （连接已保持 180s 证明流通）/ 探针从「不可达」跃迁到「可达」。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::docker::{
    follow_logs, probe_container, retry_delay, ContainerReach, LogStreamEnd, IDLE_RETRY_DELAY,
    KEEPALIVE_INTERVAL, LOG_IDLE_TIMEOUT,
};
use crate::log_status::{NOTICE_INTERRUPTED, NOTICE_RECOVERED, NOTICE_STOPPED};
use crate::models::Msg;

/// 连续失败预算：超过后转低频保活（1+2+4+8+10*4 ≈ 55s 后降到 30s 一次）
pub const MAX_RETRY_ATTEMPTS: u32 = 8;

/// 一条日志流的运行时句柄
pub struct FollowHandle {
    /// 取消信号：drop 即取消该流（分组合并日志的 N 条流共享同一个 sender）
    pub cancel: tokio::sync::watch::Sender<()>,
    /// 每条 stream 的存活标志：全部退出才算流结束（每流独立，旧流不影响新流）
    pub active: Vec<Arc<AtomicBool>>,
}

/// 流结束后的处置
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowAction {
    /// 结束流（取消 / 容器停止或删除 / 致命错误）
    End,
    /// 可恢复：退避后重连，并提示用户「正在重连」
    Recover { reason: String },
    /// 保活：静默重连（空闲超时），不打扰用户、不计失败预算
    KeepAlive { reason: String },
}

/// 续流 tail：历史未加载时拉最近 200 行；已加载过则只接续新行，避免 Clear 后历史复活。
pub fn follow_tail(history_done: bool) -> i64 {
    if history_done {
        0
    } else {
        200
    }
}

/// 结局分类：结合探针结论决定结束 / 重连 / 保活
pub fn classify_end(end: &LogStreamEnd, reach: ContainerReach) -> FollowAction {
    match end {
        LogStreamEnd::Cancelled => FollowAction::End,
        LogStreamEnd::Idle => FollowAction::KeepAlive {
            reason: format!("log stream idle timeout ({}s)", LOG_IDLE_TIMEOUT.as_secs()),
        },
        LogStreamEnd::Eof => match reach {
            // 容器已停止 / 已删除：日志流本就该结束
            ContainerReach::Stopped | ContainerReach::Gone => FollowAction::End,
            // 仍在运行却 EOF：可恢复
            ContainerReach::Running => FollowAction::Recover {
                reason: "log stream closed unexpectedly".into(),
            },
            // Docker 整体不可达：等它回来
            ContainerReach::Unreachable => FollowAction::Recover {
                reason: "docker unreachable".into(),
            },
        },
        LogStreamEnd::Error(e) => {
            if e.is_fatal() {
                // 容器已被删除：重连多少次都没用
                FollowAction::End
            } else {
                match reach {
                    ContainerReach::Running | ContainerReach::Unreachable => {
                        FollowAction::Recover {
                            reason: e.message.clone(),
                        }
                    }
                    ContainerReach::Stopped | ContainerReach::Gone => FollowAction::End,
                }
            }
        }
    }
}

/// 启动日志流运行时（每个目标一条流，共享同一个取消信号）。
///
/// `targets` 为 `(容器 id, 行前缀)`：单容器时前缀为空、只有一条流；
/// 分组合并日志时每个运行中成员一条流，前缀 `[容器名]`，日志面板里
/// 每行带来源标记。
///
/// 状态提示（重连中 / 已恢复 / 已停止）只在**单流**时上报：多路并发下
/// 任一路的结局都代表不了整组（一个成员停掉就会弹出「日志流已停止」→ 误导）。
///
/// 探针（`probe_container`）在每次非取消的结局后执行，回答两个问题：
/// 容器是否还在跑、Docker 是否可达。这是判定「该重连还是该结束」的**唯一**依据。
pub fn spawn_follow(
    rt: &Runtime,
    targets: Vec<(String, String)>,
    tail: i64,
    epoch: u64,
    tx: Sender<Msg>,
) -> FollowHandle {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(());
    // 单流才提示状态（共享 epoch 下多路状态会互相覆盖）
    let notify_status = targets.len() == 1;
    let mut actives = Vec::with_capacity(targets.len());

    for (id, prefix) in targets {
        let active = Arc::new(AtomicBool::new(true));
        actives.push(active.clone());
        let handle = rt.handle().clone();
        let tx = tx.clone();
        let cancel_rx = cancel_rx.clone();
        handle.spawn(run_stream(
            id,
            prefix,
            tail,
            epoch,
            tx,
            cancel_rx,
            notify_status,
            active,
        ));
    }

    FollowHandle {
        cancel: cancel_tx,
        active: actives,
    }
}

/// 单条流的运行时主循环：建流 → 结局分类 → 探针 → 退避重连 / 保活 / 结束
#[allow(clippy::too_many_arguments)]
async fn run_stream(
    id: String,
    prefix: String,
    tail: i64,
    epoch: u64,
    tx: Sender<Msg>,
    mut cancel_rx: tokio::sync::watch::Receiver<()>,
    notify_status: bool,
    active: Arc<AtomicBool>,
) {
    // 连续失败数：连接真正跑通（收到日志行）才归零
    let mut attempt: u32 = 0;
    // 本流是否已推送过日志：决定重连时是否重放历史（避免重复行）
    let mut sent_any = false;
    let mut retry_tail = tail;
    // 是否已经向用户提示过「正在重连」
    let mut notified = false;
    // 上一次探针是否判定 Docker 不可达（用于「不可达 → 可达」跃迁提示）
    let mut was_unreachable = false;
    // 状态提示统一经此出口：多流合并时不打扰用户（notify_status = false）
    let tell = |text: &str| {
        if notify_status {
            let _ = tx.send(Msg::LogStreamStatus(epoch, text.to_string()));
        }
    };

    loop {
        // 仅在「正在重连」状态下，本次尝试收到首行日志时提示已恢复
        let outcome = follow_logs(
            &id,
            &prefix,
            tx.clone(),
            retry_tail,
            epoch,
            cancel_rx.clone(),
            notified,
        )
        .await;
        sent_any |= outcome.lines > 0;

        // 收到日志 = 这条连接是通的 → 连续失败数归零
        if outcome.lines > 0 {
            attempt = 0;
        }
        if outcome.notified_recovered {
            // follow_logs 已提示「已恢复连接」：回到正常态
            notified = false;
        }

        // 用户取消（切容器 / Clear）：不提示，直接退出
        if matches!(outcome.end, LogStreamEnd::Cancelled) {
            break;
        }

        // 探针：容器是否还活着 / Docker 是否可达
        let reach = probe_container(&id).await;

        match classify_end(&outcome.end, reach) {
            FollowAction::End => {
                // 结束一律提示「已停止」（取消已在上面提前返回）
                tell(&crate::i18n::t(NOTICE_STOPPED));
                break;
            }
            FollowAction::KeepAlive { reason } => {
                // 空闲超时：连接已保持 LOG_IDLE_TIMEOUT 证明流通 → 视作已恢复
                if notified {
                    notified = false;
                    tell(&crate::i18n::t(NOTICE_RECOVERED));
                }
                // 连接保持过 180s，不算失败：重置预算
                attempt = 0;
                eprintln!(
                    "[log-stream] {id} idle, silent reconnect in {IDLE_RETRY_DELAY:?}: {reason}"
                );
                // 静默重连：不打扰用户
                tokio::select! {
                    _ = cancel_rx.changed() => break,
                    _ = tokio::time::sleep(IDLE_RETRY_DELAY) => {}
                }
            }
            FollowAction::Recover { reason } => {
                // 探针跃迁：Docker 从不可达恢复 → 直接提示已恢复
                let recovered = was_unreachable && reach == ContainerReach::Running;
                was_unreachable = reach == ContainerReach::Unreachable;

                if recovered {
                    notified = false;
                    tell(&crate::i18n::t(NOTICE_RECOVERED));
                } else if !notified {
                    notified = true;
                    tell(&crate::i18n::t(NOTICE_INTERRUPTED));
                }

                // 连续失败预算耗尽 → 转低频保活，**不放弃跟随**
                let delay = if attempt >= MAX_RETRY_ATTEMPTS {
                    KEEPALIVE_INTERVAL
                } else {
                    let d = retry_delay(attempt);
                    attempt += 1;
                    d
                };
                // 已推送过日志则只接续新行，避免重放历史造成重复
                if sent_any {
                    retry_tail = 0;
                }
                eprintln!(
                    "[log-stream] {id} reconnecting in {delay:?}: {reason} (attempt {attempt})"
                );

                // 退避等待可被取消：切容器/Clear 时立即退出
                tokio::select! {
                    _ = cancel_rx.changed() => break,
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }
    }

    active.store(false, Ordering::Release);
    // 流结束通知 consumer；consumer 在**全部** stream 都退出后才释放跟随状态
    let _ = tx.send(Msg::LogStreamEnded(epoch));
}

#[cfg(test)]
mod tests;
