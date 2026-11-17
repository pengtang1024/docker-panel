//! UI 应用节流窗口（从 consumer.rs 拆出，R2 行数预算）
//!
//! 只负责「本轮 pending 该用多宽的窗口」与「下一轮 recv_timeout 等多久」，
//! consumer 主循环负责 drain 与实际 apply。

use std::time::Duration;

use crate::ui_update::DrainResult;

/// 含日志批的 UI 刷新最短间隔：日志洪峰下把上千次 invoke 合并成每秒 ~8 次
pub(super) const UI_MIN_INTERVAL: Duration = Duration::from_millis(120);

/// 不含日志的批（列表 / 抽屉 / 状态栏 / 弹窗）的最小间隔。
///
/// 这一档要压的只是「同一时刻到达的多条 Msg」：列表本身只在 Docker 事件或
/// 6s stats 时变化，事件又已被 `event_watch` 的 400ms 合并窗口收敛，再压到
/// 120ms 只会让首屏的镜像/网络/卷、以及事件触发的列表更新白等一个窗口
/// （实测首屏镜像行因此从 152ms 漂到 272ms）。
pub(super) const NON_LOG_MIN_INTERVAL: Duration = Duration::from_millis(8);
/// 纯日志批的 UI 刷新间隔：日志行只影响日志视口，放宽到 250ms
/// （跟随滞后上限 ~250ms；其它任何 UI 变化仍走 UI_MIN_INTERVAL）
pub(super) const LOG_UI_MIN_INTERVAL: Duration = Duration::from_millis(250);
/// 锁外阻塞超时：兼顾退出检查与空闲时尽快 flush pending
pub(super) const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// 本轮 pending 适用的 UI 节流窗口，按「有没有日志行」分三档：
/// 纯日志 250ms（只有日志视口变）/ 日志混批 120ms（洪峰主力防护）/
/// 不含日志 8ms（只合并同一时刻到达的多条 Msg）。
pub(super) fn apply_interval(pending: &DrainResult) -> Duration {
    if pending.logs_only() {
        LOG_UI_MIN_INTERVAL
    } else if pending.is_set(crate::ui_update::UiFlag::LogsChanged) {
        UI_MIN_INTERVAL
    } else {
        NON_LOG_MIN_INTERVAL
    }
}

/// 下一轮 `recv_timeout` 的等待时间。
///
/// 有 pending 时只睡到节流窗口结束（最多再等 RECV_TIMEOUT，保证 shutdown 可观测）；
/// 空闲时固定 RECV_TIMEOUT。这样最坏 UI 延迟从「两个 100ms 超时」收敛到约 120ms。
pub(super) fn poll_timeout(
    has_pending: bool,
    interval: Duration,
    since_apply: Duration,
) -> Duration {
    if has_pending {
        interval.saturating_sub(since_apply).min(RECV_TIMEOUT)
    } else {
        RECV_TIMEOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_update::UiFlag;

    #[test]
    fn idle_uses_recv_timeout() {
        assert_eq!(
            poll_timeout(false, UI_MIN_INTERVAL, Duration::ZERO),
            RECV_TIMEOUT
        );
        assert_eq!(
            poll_timeout(false, UI_MIN_INTERVAL, Duration::from_millis(500)),
            RECV_TIMEOUT
        );
    }

    #[test]
    fn pending_wait_is_capped_by_recv_timeout() {
        assert_eq!(
            poll_timeout(true, UI_MIN_INTERVAL, Duration::ZERO),
            RECV_TIMEOUT
        );
        assert_eq!(
            poll_timeout(true, LOG_UI_MIN_INTERVAL, Duration::ZERO),
            RECV_TIMEOUT
        );
    }

    #[test]
    fn pending_wait_tracks_remaining_throttle() {
        assert_eq!(
            poll_timeout(true, UI_MIN_INTERVAL, Duration::from_millis(100)),
            Duration::from_millis(20)
        );
        assert_eq!(
            poll_timeout(true, LOG_UI_MIN_INTERVAL, Duration::from_millis(200)),
            Duration::from_millis(50)
        );
    }

    #[test]
    fn due_pending_returns_zero() {
        assert_eq!(
            poll_timeout(true, UI_MIN_INTERVAL, UI_MIN_INTERVAL),
            Duration::ZERO
        );
        assert_eq!(
            poll_timeout(true, UI_MIN_INTERVAL, Duration::from_millis(200)),
            Duration::ZERO
        );
    }

    #[test]
    fn logs_only_uses_wide_window() {
        let mut d = DrainResult::default();
        d.mark(UiFlag::LogsChanged);
        assert_eq!(apply_interval(&d), LOG_UI_MIN_INTERVAL);
        // 日志 + 其它变化混批：退回 120ms（洪峰主力防护）
        d.docker_ok = Some(true);
        assert_eq!(apply_interval(&d), UI_MIN_INTERVAL);
        d.docker_ok = None;
        d.toast_text = Some("x".into());
        assert_eq!(apply_interval(&d), UI_MIN_INTERVAL);
        // 没有日志变化（例如只有列表）走 8ms 窄窗口，不再白等一个 120ms 窗口
        let mut d2 = DrainResult::default();
        d2.mark(UiFlag::ContainersChanged);
        assert_eq!(apply_interval(&d2), NON_LOG_MIN_INTERVAL);
    }
}
