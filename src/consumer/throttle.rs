//! UI 应用节流窗口（从 consumer.rs 拆出，R2 行数预算）
//!
//! 只负责「本轮 pending 该用多宽的窗口」与「下一轮 recv_timeout 等多久」，
//! consumer 主循环负责 drain 与实际 apply。

use std::time::Duration;

use crate::ui_update::DrainResult;

/// UI 刷新最短间隔：日志洪峰下把上千次 invoke 合并成每秒 ~8 次
pub(super) const UI_MIN_INTERVAL: Duration = Duration::from_millis(120);
/// 纯日志批的 UI 刷新间隔：日志行只影响日志视口，放宽到 250ms
/// （跟随滞后上限 ~250ms；其它任何 UI 变化仍走 UI_MIN_INTERVAL）
pub(super) const LOG_UI_MIN_INTERVAL: Duration = Duration::from_millis(250);
/// 锁外阻塞超时：兼顾退出检查与空闲时尽快 flush pending
pub(super) const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// 本轮 pending 适用的 UI 节流窗口：只有日志行变化时用宽松窗口，
/// 其余情况（列表 / 抽屉 / 状态栏 / 弹窗）一律 120ms。
pub(super) fn apply_interval(pending: &DrainResult) -> Duration {
    if pending.logs_only() {
        LOG_UI_MIN_INTERVAL
    } else {
        UI_MIN_INTERVAL
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
        let mut d = DrainResult {
            logs_changed: true,
            ..Default::default()
        };
        assert_eq!(apply_interval(&d), LOG_UI_MIN_INTERVAL);
        // 任何其它 UI 变化都退回常规窗口
        d.docker_ok = Some(true);
        assert_eq!(apply_interval(&d), UI_MIN_INTERVAL);
        d.docker_ok = None;
        d.status_text = Some("x".into());
        assert_eq!(apply_interval(&d), UI_MIN_INTERVAL);
        // 没有日志变化（例如只有列表）也走常规窗口
        let d2 = DrainResult {
            containers_changed: true,
            ..Default::default()
        };
        assert_eq!(apply_interval(&d2), UI_MIN_INTERVAL);
    }
}
