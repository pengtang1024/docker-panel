//! 日志相关消息的应用：批量行 / 流结束 / 流状态提示
//!
//! 从 `ui_update.rs` 拆出（R2：正文 ≤300 行）。只处理 `Msg::LogBatch` /
//! `Msg::LogStreamEnded` / `Msg::LogStreamStatus` 三种消息，资源列表与操作类消息
//! 仍留在父模块的 match 里。

use crate::ui_state::{AppState, LOG_ROWS_MAX};

use super::{DrainResult, UiFlag};

impl AppState {
    /// 一批日志行落地：先归还积压计账，再按流 epoch 过滤
    pub(super) fn on_log_batch(
        &mut self,
        epoch: u64,
        mut lines: Vec<String>,
        res: &mut DrainResult,
    ) {
        // 积压计账要先扣（旧流的排队行同样要归零，否则生产者会一直以为 UI 跟不上）
        self.drain_backlog(lines.len() as u64);
        // 只接受当前流 epoch：切换容器/清空后，队列里旧流的行直接丢弃
        if epoch != self.stream_epoch {
            return;
        }
        // 洪峰下单批可达数万行：只保留最后 LOG_ROWS_MAX 行。
        // 中间行既显示不出来（全被新行挤出视口），保留下来还会把
        // 行模型撑到十万级 —— 裁剪退化成 O(n²)，洪峰下直接打满 CPU。
        let start = lines.len().saturating_sub(LOG_ROWS_MAX);
        // 整批搬进窗口（移动，不是拷贝）：洪峰下逐行拷贝会打满 CPU
        self.logs_total += (lines.len() - start) as u64;
        self.logs.extend(lines.drain(start..));
        self.trim_logs();
        // 收到首条（历史）日志后才算历史加载完成；空历史/首流失败可重试 tail=200
        self.history_done = true;
        res.mark(UiFlag::LogsChanged);
    }

    /// 一条流结束：分组合并日志有 N 条流，只有**全部**退出才算整体结束
    pub(super) fn on_log_stream_ended(&mut self, epoch: u64) {
        if epoch == self.stream_epoch && !self.follow_is_alive() {
            self.follow_id.clear();
            self.follow_targets.clear();
            self.follow_cancel = None;
            self.follow_active.clear();
        }
    }

    /// 日志流状态提示（重连中 / 已恢复 / 已停止 / 洪峰抽样）：独立通道，不占状态栏
    pub(super) fn on_log_stream_status(&mut self, epoch: u64, text: String, res: &mut DrainResult) {
        // 运行时就只在状态**变化**时发消息，这里直接透传给 UI
        if epoch == self.stream_epoch {
            res.log_status = Some(text);
        }
    }
}
