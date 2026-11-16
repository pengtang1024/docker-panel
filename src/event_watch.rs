//! Docker 事件订阅：列表即时性的唯一来源。空闲时零请求，资源真正变化时立刻刷新。
//!
//! 纪律：
//! - 只看 `type` = container/image/network/volume；动作走**噪声黑名单**，未知动作一律刷新
//!   （漏刷才是 bug：陈旧数据没人发现；多刷只多一次 ≈170ms 的 list 调用）
//! - 400ms 合并窗口：突发启停 / compose up 只刷一遍
//! - 断流指数退避重连（1s→30s）永不放弃
//! - **连接建立即补一次全量刷新**：首次连接成功（dockerd 后起）、断流后重连成功、
//!   连续失败后成功都算。空闲流不会给我们任何事件，靠事件驱动的话列表永远补不回来。
//!
//! 不做轮询兜底：`/events` 出问题会在这里报错（`[events] stream error`），
//! 让根因暴露出来，而不是被一个定时器悄悄抹平成「最多慢 15s」。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use bollard::models::{EventMessage, EventMessageTypeEnum};
use bollard::query_parameters::EventsOptionsBuilder;
use futures_util::StreamExt;

use crate::ui_state::refresh::Kinds;
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
        let mut conn = ConnState::default();
        loop {
            match watch(&state_t, &mut conn).await {
                WatchEnd::Shutdown => return,
                WatchEnd::Failed => {
                    conn.on_disconnected();
                    attempt = attempt.saturating_add(1);
                }
                WatchEnd::Ended => {
                    // 能建立连接就不该继承上一轮的惩罚，退避归零
                    conn.on_disconnected();
                    attempt = 0;
                }
            }
            tokio::time::sleep(backoff(attempt)).await;
        }
    });
}

/// 一次事件流的结局。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WatchEnd {
    /// 连接建立后断流 / 被 Docker 关闭（走重连）
    Ended,
    /// 连不上（Docker 不可达），保持退避
    Failed,
    /// 应用正在退出
    Shutdown,
}

/// 连接状态机：把「连接建立」的判定集中在一处，保证每次跃迁只补一次刷新。
///
/// `connected` 由 `watch` 在流结束后置 false；`on_connected` 返回 true 表示
/// 这是「从不可用变为可用」的那一次（含首次成功），调用方据此补一次全量刷新。
#[derive(Default)]
struct ConnState {
    connected: bool,
}

impl ConnState {
    fn on_connected(&mut self) -> bool {
        let was_down = !self.connected;
        self.connected = true;
        was_down
    }

    fn on_disconnected(&mut self) {
        self.connected = false;
    }
}

/// 消费一次事件流。
///
/// 先用 `ping` 明确区分「连上了」与「连不上」——只有前者算连接建立，
/// 这正是 dockerd 后起 / 断流恢复时列表能自己补回来的关键：空闲的事件流
/// 永远不会给我们任何事件，不能靠事件驱动来发现问题。
async fn watch(state: &SharedState, conn: &mut ConnState) -> WatchEnd {
    let Ok(docker) = crate::docker::connect() else {
        return WatchEnd::Failed;
    };
    if docker.ping().await.is_err() {
        eprintln!("[events] ping failed");
        return WatchEnd::Failed;
    }
    if conn.on_connected() {
        // 断流窗口内的事件永久丢失 / 首屏可能早于 dockerd 就绪：
        // 连接建立的这一刻状态才是真相，先补一次全量刷新再消费事件。
        if let Some(mut s) = crate::lock::acquire(state, "event_watch") {
            s.refresh();
        }
    }

    let mut filters = HashMap::new();
    filters.insert("type", vec!["container", "image", "network", "volume"]);
    let opts = EventsOptionsBuilder::new().filters(&filters).build();
    let mut stream = docker.events(Some(opts));
    let mut db = Debouncer::default();

    loop {
        // 有被合并窗压住的事件时，同时等「新事件」与「窗口到期」，谁先到算谁：
        // 到期必须补刷一次，否则窗口尾部那些事件改变的状态会一直缺席到下次事件。
        let item = match db.deadline() {
            Some(deadline) => {
                let sleep = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
                tokio::pin!(sleep);
                tokio::select! {
                    item = stream.next() => item,
                    _ = &mut sleep => {
                        let kinds = db.on_flushed(Instant::now());
                        if !shutdown(state) {
                            if let Some(mut s) = crate::lock::acquire(state, "event_watch") {
                                s.refresh_kinds(kinds);
                            }
                        }
                        continue;
                    }
                }
            }
            None => stream.next().await,
        };
        let Some(item) = item else { break };
        if shutdown(state) {
            return WatchEnd::Shutdown;
        }
        let kinds = match item {
            Ok(ev) if is_noise(&ev) => continue,
            Ok(ev) => kinds_of(&ev),
            Err(e) => {
                eprintln!("[events] stream error: {e}");
                return WatchEnd::Ended;
            }
        };
        if let Some(kinds) = db.on_event(Instant::now(), kinds) {
            if let Some(mut s) = crate::lock::acquire(state, "event_watch") {
                s.refresh_kinds(kinds);
            }
        }
    }
    WatchEnd::Ended
}

/// 事件合并窗口的状态机：窗口内首事件立即刷新（leading edge，动作反馈要快），
/// 窗口内后续事件**记账**而不是丢弃 —— 窗口到期由事件循环补刷一次（trailing edge）。
///
/// 曾经只丢不补：`start` 之后 400ms 内到达的 `stop` 会被吞掉，列表和真实状态
/// 一直不一致到下一个事件为止（同一窗口里只有「最后一次变化」是真相）。
#[derive(Default)]
struct Debouncer {
    /// 上一次触发刷新（或窗口到期）的时刻。
    last: Option<Instant>,
    /// 窗口内是否有被压住的事件（有 → 到期要补刷）。
    pending: bool,
    /// 窗口内被压住的事件需要刷新的资源种类（到期补刷用）。
    kinds: Kinds,
}

impl Debouncer {
    /// 收到一个非噪声事件：Some(kinds) = 立刻刷新这批资源，None = 已被窗口吸收、等到期。
    fn on_event(&mut self, now: Instant, kinds: Kinds) -> Option<Kinds> {
        match self.last {
            Some(t) if now.duration_since(t) < DEBOUNCE => {
                self.pending = true;
                self.kinds.insert(kinds);
                None
            }
            _ => {
                self.last = Some(now);
                self.pending = false;
                Some(kinds)
            }
        }
    }

    /// 有待补刷时返回窗口到期的时刻（事件循环据此 `select!` 一个定时器）。
    fn deadline(&self) -> Option<Instant> {
        if self.pending {
            self.last.map(|t| t + DEBOUNCE)
        } else {
            None
        }
    }

    /// 窗口到期、补刷完成：返回窗口内累计的资源种类，并从这一刻重新开窗。
    fn on_flushed(&mut self, now: Instant) -> Kinds {
        self.last = Some(now);
        self.pending = false;
        std::mem::replace(&mut self.kinds, Kinds::NONE)
    }
}

/// 事件类型 → 需要刷新的资源（只刷变化的那一路：容器启停不必重拉镜像 / 网络 / 卷）。
///
/// 镜像的「被引用容器数」不在这里补：容器列表到达后由应用侧本地重算，
/// 比再发一次 `/images/json`（≈50ms）便宜得多。
fn kinds_of(ev: &EventMessage) -> Kinds {
    match ev.typ {
        Some(EventMessageTypeEnum::CONTAINER) => Kinds::CONTAINERS,
        Some(EventMessageTypeEnum::IMAGE) => Kinds::IMAGES,
        Some(EventMessageTypeEnum::NETWORK) => Kinds::NETWORKS,
        Some(EventMessageTypeEnum::VOLUME) => Kinds::VOLUMES,
        // 过滤条件已限定四种类型，走到这里说明 Docker 给了新类型：宁可全量
        _ => Kinds::ALL,
    }
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

    /// 窗口内首事件立即刷新：动作反馈要快（领先沿）。
    #[test]
    fn first_event_refreshes_immediately() {
        let mut d = Debouncer::default();
        let t0 = Instant::now();
        assert_eq!(d.on_event(t0, Kinds::CONTAINERS), Some(Kinds::CONTAINERS));
        assert_eq!(d.deadline(), None, "没有待补刷事件时不应排定时器");
    }

    /// 窗口内后续事件必须记账并排出到期定时器 —— 这是曾经丢了刷新的场景：
    /// `start` 后 400ms 内的 `stop` 若被吞掉，列表会一直停在 running。
    #[test]
    fn suppressed_event_schedules_trailing_flush() {
        let mut d = Debouncer::default();
        let t0 = Instant::now();
        assert_eq!(d.on_event(t0, Kinds::CONTAINERS), Some(Kinds::CONTAINERS));
        assert_eq!(
            d.on_event(t0 + Duration::from_millis(100), Kinds::CONTAINERS),
            None,
            "窗口内应被吸收"
        );
        assert_eq!(d.deadline(), Some(t0 + DEBOUNCE), "必须排到期补刷");
        // 窗口内混入另一种资源：到期时要一起刷（种类必须累加，不能只留最后一条）
        assert_eq!(
            d.on_event(t0 + Duration::from_millis(300), Kinds::IMAGES),
            None
        );
        assert_eq!(d.deadline(), Some(t0 + DEBOUNCE), "多处事件共用同一窗口");

        let flushed = d.on_flushed(t0 + DEBOUNCE);
        assert!(flushed.contains(Kinds::CONTAINERS) && flushed.contains(Kinds::IMAGES));
        assert!(d.on_flushed(t0 + DEBOUNCE).is_empty(), "取走后再取应为空");
        assert_eq!(d.deadline(), None, "补刷后窗口重置");
        assert_eq!(
            d.on_event(t0 + DEBOUNCE + DEBOUNCE, Kinds::VOLUMES),
            Some(Kinds::VOLUMES),
            "补刷后从该刻重新开窗，窗口外的下一个事件立即刷新"
        );
    }

    /// 窗口外的第一个事件立即刷新，不需要等定时器。
    #[test]
    fn event_after_window_refreshes_immediately() {
        let mut d = Debouncer::default();
        let t0 = Instant::now();
        assert_eq!(d.on_event(t0, Kinds::CONTAINERS), Some(Kinds::CONTAINERS));
        assert_eq!(
            d.on_event(t0 + DEBOUNCE, Kinds::CONTAINERS),
            Some(Kinds::CONTAINERS)
        );
        assert_eq!(d.deadline(), None);
    }

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

    /// 每次 `on_connected` 是否要补刷新（1 = 要，0 = 已在连接中）
    fn refresh_count(conn: &mut ConnState) -> usize {
        conn.on_connected() as usize
    }

    #[test]
    fn first_connect_success_refreshes_exactly_once() {
        let mut conn = ConnState::default();
        assert_eq!(refresh_count(&mut conn), 1, "首次连接成功必须补一次刷新");
        // 同一条连接内重复判定不再补
        assert_eq!(refresh_count(&mut conn), 0);
    }

    #[test]
    fn reconnect_after_stream_end_refreshes_exactly_once() {
        let mut conn = ConnState::default();
        let mut total = refresh_count(&mut conn); // 首连成功
        conn.on_disconnected(); // 断流
        total += refresh_count(&mut conn);
        conn.on_disconnected();
        total += refresh_count(&mut conn);
        assert_eq!(total, 3, "两次断流重连各补一次，且每次跃迁只触发一次");
    }

    #[test]
    fn consecutive_failures_then_success_refreshes_exactly_once() {
        let mut conn = ConnState::default();
        // 连续失败：连不上，不补刷新
        for _ in 0..3 {
            conn.on_disconnected();
        }
        assert_eq!(refresh_count(&mut conn), 1, "连续失败后的首次成功只补一次");
        // 失败不会累积成多次刷新
        conn.on_disconnected();
        conn.on_disconnected();
        assert_eq!(refresh_count(&mut conn), 1);
    }
}
