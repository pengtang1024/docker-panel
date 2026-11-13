//! 采样过期判定：连续多轮没有样本 → 过期，且**只在状态跃迁时**报告。
//!
//! stats 与 `/system/df` 都是「拉不到就什么都不发」的后台任务：单轮失败时
//! UI 保持旧值，用户无从判断数字是「没变」还是「已经死了」。这里给两者
//! 各一个轮次阈值，连续无样本达到阈值才点亮提示，恢复后自动熄灭。
//!
//! 边沿触发（返回 `Option<bool>`）是为了遵守规范 #34：不做定时心跳式重绘，
//! 每轮都推一条消息只会让提示 chip 反复重绘。

/// stats 连续 3 轮（6s × 3 = 18s）拿不到任何样本才算过期。
///
/// 依据：单轮失败是常态（Docker 正忙、一次 10s 超时），一轮就亮会随抖动闪烁；
/// 3 轮 ≈18s 足以过滤抖动，又能在 daemon 卡死时 20s 内被用户看见。
pub const STATS_STALE_ROUNDS: u32 = 3;

/// 磁盘占用 30s 一轮，连续 2 轮（≈60s）失败才算过期。
///
/// 依据：`/system/df` 要遍历卷目录与容器可写层，磁盘繁忙时单轮超时很常见；
/// 60s 恰好是「两轮没更新」，用户能感知到数字冻结。
pub const DISK_STALE_ROUNDS: u32 = 2;

/// 「连续无样本轮次 → 是否过期」的记忆器。
pub struct StaleTracker {
    threshold: u32,
    fail_streak: u32,
    stale: bool,
}

impl StaleTracker {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            fail_streak: 0,
            stale: false,
        }
    }

    /// 记录一轮采样：`got_sample = false` 表示这一轮没有任何样本。
    ///
    /// 返回 `Some(bool)` 仅当「过期」状态发生跃迁。
    pub fn observe(&mut self, got_sample: bool) -> Option<bool> {
        self.fail_streak = if got_sample {
            0
        } else {
            self.fail_streak.saturating_add(1)
        };
        let now_stale = self.fail_streak >= self.threshold;
        if now_stale == self.stale {
            return None;
        }
        self.stale = now_stale;
        Some(now_stale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_failed_round_does_not_flicker() {
        let mut t = StaleTracker::new(STATS_STALE_ROUNDS);
        assert_eq!(t.observe(false), None, "单次失败不能让提示闪烁");
        assert_eq!(t.observe(true), None, "恢复也应保持静默");
    }

    #[test]
    fn only_reports_transitions() {
        let mut t = StaleTracker::new(2);
        assert_eq!(t.observe(false), None);
        assert_eq!(t.observe(false), Some(true), "达到阈值时报告一次过期");
        assert_eq!(t.observe(false), None, "持续过期不重复报告");
        assert_eq!(t.observe(true), Some(false), "恢复时报告一次清除");
        assert_eq!(t.observe(true), None);
    }

    #[test]
    fn recovery_resets_the_streak() {
        let mut t = StaleTracker::new(2);
        assert_eq!(t.observe(false), None);
        assert_eq!(t.observe(true), None, "成功一轮后失败计数归零");
        assert_eq!(t.observe(false), None, "重新计数，不累积到上一轮的失败");
        assert_eq!(t.observe(false), Some(true));
    }
}
