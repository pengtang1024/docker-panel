//! Docker 事件订阅：列表即时性的唯一来源。空闲时零请求，资源真正变化时立刻刷新。
//!
//! 纪律：
//! - 只看 `type` = container/image/network/volume；动作走**噪声黑名单**，未知动作一律刷新
//!   （漏刷才是 bug：陈旧数据没人发现；多刷只多一次 ≈170ms 的 list 调用）
//! - 400ms 合并窗口：突发启停 / compose up 只刷一遍
//! - 断流指数退避重连（1s→30s）永不放弃；断流期间的窗口我们看不见，重连后补一次全量刷新
//!
//! 不做轮询兜底：`/events` 出问题会在这里报错（`[events] stream error`），
//! 让根因暴露出来，而不是被一个定时器悄悄抹平成「最多慢 15s」。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use bollard::models::{EventMessage, EventMessageTypeEnum};
use bollard::query_parameters::EventsOptionsBuilder;
use futures_util::StreamExt;

use crate::ui_state::SharedState;

/// 事件合并窗口：400ms 内连续到达的事件只触发一次刷新。
const DEBOUNCE: Duration = Duration::from_millis(400);
/// 断流重连退避上限。
const RETRY_MAX: Duration = Duration::from_secs(30);

/// 起一个常驻任务消费 `/events`。Docker 不可用时也在后台重试，不阻塞启动。
pub fn spawn(state: &SharedState) {
    let rt = {
        let Ok(s) = state.lock() else {
            crate::lock::poison("event_watch");
            return;
        };
        s.rt.handle().clone()
    };
    let state_t = state.clone();
    rt.spawn(async move {
        let mut attempt = 0u32;
        loop {
            let live = watch(&state_t).await;
            if shutdown(&state_t) {
                return;
            }
            if live {
                // 这条流断掉的瞬间起我们就是瞎的：重连前补一次全量刷新，把断流窗口对齐。
                // 同时把退避归零（能收到事件的连接不该继承上一轮的惩罚）。
                attempt = 0;
                if let Some(mut s) = crate::lock::acquire(&state_t, "event_watch") {
                    s.refresh();
                }
            }
            attempt = attempt.saturating_add(1);
            tokio::time::sleep(backoff(attempt)).await;
        }
    });
}

/// 消费一次事件流，返回期间是否真的收到过事件——
/// 区分「连上过又断了」（需要补刷新）和「一直连不上」（没有可对齐的窗口）。
async fn watch(state: &SharedState) -> bool {
    let Ok(docker) = crate::docker::connect() else {
        return false;
    };
    let mut filters = HashMap::new();
    filters.insert("type", vec!["container", "image", "network", "volume"]);
    let opts = EventsOptionsBuilder::new().filters(&filters).build();
    let mut stream = docker.events(Some(opts));
    let mut last: Option<Instant> = None;
    let mut live = false;

    while let Some(item) = stream.next().await {
        if shutdown(state) {
            return live;
        }
        match item {
            Ok(ev) if is_noise(&ev) => continue,
            Ok(_) => live = true,
            Err(e) => {
                eprintln!("[events] stream error: {e}");
                return live;
            }
        }
        // 合并窗口内的后续事件直接吞掉（流照常读，不阻塞）
        if last.is_some_and(|t| t.elapsed() < DEBOUNCE) {
            continue;
        }
        last = Some(Instant::now());
        if let Some(mut s) = crate::lock::acquire(state, "event_watch") {
            s.refresh();
        }
    }
    live
}

/// 高频且不改变列表可见状态的事件：健康检查（每次探针都发）与 exec / attach 生命周期。
/// 其余动作一律当成「可能需要重刷」，宁可多刷一次也不漏刷。
fn is_noise(ev: &EventMessage) -> bool {
    let known_type = ev.typ.as_ref().is_some_and(|t| {
        matches!(
            t,
            EventMessageTypeEnum::CONTAINER
                | EventMessageTypeEnum::IMAGE
                | EventMessageTypeEnum::NETWORK
                | EventMessageTypeEnum::VOLUME
        )
    });
    if !known_type {
        return true;
    }
    let action = ev.action.as_deref().unwrap_or_default();
    action.is_empty()
        || action.starts_with("health_status")
        || action.starts_with("exec_")
        || action == "attach"
        || action == "detach"
}

fn shutdown(state: &SharedState) -> bool {
    state
        .lock()
        .map(|s| s.shutdown.load(Ordering::Relaxed))
        .unwrap_or_else(|_| {
            crate::lock::poison("event_watch::shutdown");
            true
        })
}

/// 1s, 2s, 4s, 8s, 16s, 30s（封顶）
fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(5)).min(RETRY_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(typ: EventMessageTypeEnum, action: &str) -> EventMessage {
        EventMessage {
            typ: Some(typ),
            action: Some(action.into()),
            ..Default::default()
        }
    }

    #[test]
    fn known_actions_refresh() {
        assert!(!is_noise(&ev(EventMessageTypeEnum::CONTAINER, "start")));
        assert!(!is_noise(&ev(EventMessageTypeEnum::CONTAINER, "destroy")));
        assert!(!is_noise(&ev(EventMessageTypeEnum::IMAGE, "tag")));
        assert!(!is_noise(&ev(EventMessageTypeEnum::VOLUME, "create")));
    }

    #[test]
    fn unknown_actions_still_refresh() {
        // 黑名单的意义：Docker 新增或我们没列到的动作不能静默漏掉
        assert!(!is_noise(&ev(EventMessageTypeEnum::CONTAINER, "commit")));
        assert!(!is_noise(&ev(EventMessageTypeEnum::CONTAINER, "prune")));
    }

    #[test]
    fn ignores_noise() {
        // health_status 每次健康检查都会发，不能按它的节奏重刷整个台账
        assert!(is_noise(&ev(
            EventMessageTypeEnum::CONTAINER,
            "health_status: healthy"
        )));
        assert!(is_noise(&ev(EventMessageTypeEnum::CONTAINER, "exec_start")));
        assert!(is_noise(&ev(EventMessageTypeEnum::CONTAINER, "attach")));
        assert!(is_noise(&ev(EventMessageTypeEnum::DAEMON, "start")));
        assert!(is_noise(&ev(EventMessageTypeEnum::CONTAINER, "")));
    }

    #[test]
    fn backoff_is_capped() {
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(5), RETRY_MAX);
        assert_eq!(backoff(99), RETRY_MAX);
    }
}
