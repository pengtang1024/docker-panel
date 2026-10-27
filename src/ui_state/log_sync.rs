//! 日志窗口裁剪 + UI 日志行模型同步账
//!
//! 行模型（UI 线程里的 `VecModel<LogRow>`）是日志窗口（AppState 里的 `logs`）
//! 的**镜像**：行数、顺序、内容完全一致，区别只在同步方式：
//! - 追加：已有行索引不变 → 虚拟化 ListView 的可见行不必重排（日志流不卡的关键）；
//! - 整体重建：整表替换，可见行重排一次（每裁剪一轮一次）。
//!
//! 两者用「全局行流位置」对齐：
//! - 窗口 = 全局行流的 `[logs_total - logs.len(), logs_total)` 段；
//! - 模型 = 全局行流的 `[model_total - model_rows, model_total)` 段。
//!
//! 起点相同 → 模型是窗口的前缀，只需追加尾部新行；
//! 起点不同 → 窗口裁剪过前段（或换了流），模型无法增量对齐，只能整体重建。

use super::AppState;

/// 日志缓冲与 UI 日志行模型共用的保留上限（行）。
///
/// 缓冲（AppState）与行模型（UI 线程）必须用同一个上限与裁剪规则，
/// 否则「复制日志」的内容会和日志面板看到的不一致。
pub const LOG_ROWS_MAX: usize = 2000;
/// 触发裁剪后保留的行数：分块裁剪（一次丢掉一整段旧行）。
///
/// 不能逐行裁剪：环形缓冲每来一行就把最旧一行挤掉，整表索引全部错位，
/// Slint 的 ListView 会把所有可见行标记为脏并重新排版（实测是日志卡顿主因）。
/// 分块裁剪只在攒够一段时整体搬移一次，代价是每 `LOG_ROWS_MAX - LOG_ROWS_KEEP`
/// 行一次「模型整体重建」（可见行重排一次），摊到每行可忽略。
pub const LOG_ROWS_KEEP: usize = 1000;

/// 日志行模型与日志窗口的对齐方案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSyncPlan {
    /// 模型现有内容是窗口内容的前缀：只需把窗口尾部 `from` 起的新行追加到模型尾部。
    Append { from: usize },
    /// 模型与窗口已错位（窗口裁剪过前段 / 换了流）：必须整体重建。
    Reset,
}

impl AppState {
    /// 分块裁剪日志窗口：超过上限时一次性丢掉最旧的一段。
    ///
    /// 与 UI 日志行模型（`render::logs_sync`）用同一套上限与裁剪规则，
    /// 保证「复制日志」的内容与日志面板一致。
    pub fn trim_logs(&mut self) {
        if self.logs.len() > self.logs_cap {
            let excess = self.logs.len() - LOG_ROWS_KEEP;
            self.logs.drain(0..excess);
        }
    }

    /// 计算 UI 日志行模型相对日志窗口的同步方案。
    pub fn log_sync_plan(&self) -> LogSyncPlan {
        // 模型属于另一条流（Clear / 切容器）：内容不可信，整体重建
        if self.model_epoch != self.stream_epoch {
            return LogSyncPlan::Reset;
        }
        let window_start = self.logs_total - self.logs.len() as u64;
        let model_start = self.model_total - self.model_rows as u64;
        if model_start != window_start {
            return LogSyncPlan::Reset;
        }
        LogSyncPlan::Append {
            from: self.model_total.saturating_sub(window_start) as usize,
        }
    }

    /// 记录一次同步结果（行模型已与日志窗口对齐）
    pub fn commit_log_sync(&mut self) {
        self.model_total = self.logs_total;
        self.model_rows = self.logs.len();
        self.model_epoch = self.stream_epoch;
    }

    /// 清空行模型的同步账：模型已随面板清空，从当前位置重新开始对齐
    pub(super) fn reset_log_model_tracking(&mut self) {
        self.model_total = self.logs_total;
        self.model_rows = 0;
        self.model_epoch = self.stream_epoch;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::models::Msg;

    /// 灌入一批日志行（走 Msg 通道，epoch 与当前流一致）
    fn feed(s: &mut AppState, rx: &mpsc::Receiver<Msg>, n: usize) {
        let tx = s.tx.clone();
        let epoch = s.stream_epoch;
        let lines: Vec<String> = (0..n).map(|i| format!("line-{i}")).collect();
        tx.send(Msg::LogBatch(epoch, lines)).unwrap();
        s.drain_messages(rx, None, 10);
    }

    #[test]
    fn log_sync_plan_appends_only_new_lines() {
        let (state, rx) = AppState::new();
        let mut s = state.lock().unwrap();

        // 模型还是空的：窗口里的内容全部是新增
        feed(&mut s, &rx, 3);
        assert_eq!(s.log_sync_plan(), LogSyncPlan::Append { from: 0 });
        s.commit_log_sync();

        // 同步之后：只追加尾部新增的行，已有行索引不变
        feed(&mut s, &rx, 2);
        assert_eq!(s.log_sync_plan(), LogSyncPlan::Append { from: 3 });
        s.commit_log_sync();
        assert_eq!(s.logs.len(), 5);
        assert_eq!(s.model_rows, 5);
    }

    #[test]
    fn log_sync_plan_resets_after_window_trim() {
        let (state, rx) = AppState::new();
        let mut s = state.lock().unwrap();

        // 灌到上限：窗口停在上限行（分块裁剪的边界）
        feed(&mut s, &rx, LOG_ROWS_MAX + 1);
        assert_eq!(s.logs.len(), LOG_ROWS_MAX);
        s.commit_log_sync();

        // 再来一行 → 窗口裁掉最旧的一段，起点前移 → 模型无法增量对齐
        feed(&mut s, &rx, 1);
        assert_eq!(s.logs.len(), LOG_ROWS_KEEP, "窗口分块裁剪到保留行数");
        assert_eq!(s.log_sync_plan(), LogSyncPlan::Reset);

        // 重建之后又回到追加路径
        s.commit_log_sync();
        feed(&mut s, &rx, 1);
        assert_eq!(
            s.log_sync_plan(),
            LogSyncPlan::Append {
                from: LOG_ROWS_KEEP
            }
        );
    }

    #[test]
    fn log_sync_plan_resets_after_clear() {
        let (state, rx) = AppState::new();
        let mut s = state.lock().unwrap();
        feed(&mut s, &rx, 4);
        s.commit_log_sync();

        // Clear：缓冲与模型同时清空，从当前位置重新对齐
        s.clear_log_buffer();
        assert_eq!(s.logs.len(), 0);
        assert_eq!(s.model_rows, 0);
        assert_eq!(
            s.log_sync_plan(),
            LogSyncPlan::Append { from: 0 },
            "模型已清空：窗口为空，无需重建"
        );
    }

    #[test]
    fn log_sync_plan_resets_when_stream_epoch_changed() {
        let (state, rx) = AppState::new();
        let mut s = state.lock().unwrap();
        feed(&mut s, &rx, 4);
        s.commit_log_sync();

        // 换流（ensure_follow → spawn_follow）后模型内容不再可信
        s.ensure_follow("c1");
        assert_eq!(s.log_sync_plan(), LogSyncPlan::Reset);
    }
}
