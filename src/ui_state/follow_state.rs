//! 日志跟随流的 AppState 状态机（建流 / Clear / 幂等判定）
//!
//! 从 `ui_state.rs` 拆出（R2：单文件正文 ≤300 行）：本模块只负责**写状态字段**
//! 与调用 `follow_runtime`；退避重连 / 探针 / 存活标志全在 runtime 侧。
//!
//! 支持两种跟随模式：
//! - 单容器：`ensure_follow(id)`，一条流，行前缀为空；
//! - 分组：`ensure_group_follow(key, targets)`，每个运行中成员一条流，
//!   共享同一个 `stream_epoch` 与取消信号，行前缀为 `[容器名]`
//!   → 抽屉里看到的是带来源标记的合并日志。

use std::sync::atomic::Ordering;

use super::AppState;

impl AppState {
    /// 切换选中资源：清空日志缓冲，取消旧流，并让队列里旧 epoch 的日志失效。
    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.follow_id.clear();
        self.follow_targets.clear();
        self.history_done = false;
        self.log_epoch.fetch_add(1, Ordering::AcqRel);
        self.stream_epoch = self.stream_epoch.wrapping_add(1);
        // drop sender → 所有 receiver 的 changed() 立即返回，follow 任务退出
        self.follow_cancel = None;
        self.follow_active.clear();
        self.reset_log_model_tracking();
    }

    /// Clear 工具条：清空缓冲后以 tail=0 重启当前流，只保留之后的新日志。
    ///
    /// 不能只 clear 缓冲：旧流在 mpsc 队列中已排队的行会在下一批 drain 时重新出现。
    pub fn clear_log_buffer(&mut self) {
        let had_logs = !self.logs.is_empty();
        let active = !self.follow_id.is_empty();
        self.logs.clear();
        // 只有确实清掉了历史，或当前流仍在（Clear 会丢弃其排队行）时，才标记历史已完成。
        // 空缓冲 + 无活动流时保留原标志，让下次 ensure_follow 仍可 tail=200 拉启动日志。
        if had_logs || active {
            self.history_done = true;
        }
        // 即使当前没有活动流（流已自然结束），也必须推进 epoch，
        // 让已经投递但尚未执行的旧日志行 invoke 失效。
        self.log_epoch.fetch_add(1, Ordering::AcqRel);
        if !active {
            self.follow_active.clear();
            self.reset_log_model_tracking();
            return;
        }
        // 原键 + 原目标（分组日志要重建它的**全部**成员流，不能只重建第一条）
        let key = self.follow_id.clone();
        let targets = self.follow_targets.clone();
        self.spawn_follow(&key, targets, 0);
        self.reset_log_model_tracking();
    }

    /// 确保对指定容器建立日志跟随流（幂等：同一容器只建一条，避免重复行）
    ///
    /// 返回 `true` 表示**新建了一条流**：调用方需要据此清掉 UI 上的旧状态提示
    /// （新流没有「正在重连」的历史状态，旧提示会滞留误导用户）。
    pub fn ensure_follow(&mut self, id: &str) -> bool {
        // 防御调用方绕过 clear_logs 直接切到另一个容器：先清掉旧缓冲，避免串台
        if !self.follow_id.is_empty() && self.follow_id != id {
            self.clear_logs();
        }
        if self.follow_id == id && self.follow_is_alive() {
            return false;
        }
        // 历史已加载过（含 Clear 后）只接续新行；否则首次拉最近 200 行。
        // history_done 在收到首条日志或 Clear 时才置位，空历史/首流失败可重试。
        let tail = crate::follow_runtime::follow_tail(self.history_done);
        self.spawn_follow(id, vec![(id.to_string(), String::new())], tail);
        true
    }

    /// 确保对分组建立**合并日志**流（组内每个目标一条流，共享 epoch 与取消信号）。
    ///
    /// `targets` 为 `(容器 id, 行前缀)`；幂等键是分组键（`project:...` / `svc:...`），
    /// 同一分组重复点击不会重复建流。
    pub fn ensure_group_follow(&mut self, key: &str, targets: Vec<(String, String)>) -> bool {
        if !self.follow_id.is_empty() && self.follow_id != key {
            self.clear_logs();
        }
        if self.follow_id == key && self.follow_is_alive() {
            return false;
        }
        let tail = crate::follow_runtime::follow_tail(self.history_done);
        self.spawn_follow(key, targets, tail);
        true
    }

    /// 当前日志流是否仍在运行：每条流独立标志，旧流退出不会误标新流。
    ///
    /// 分组合并日志有 N 条流：**全部**退出才算结束（任一条还在跑就继续跟随）。
    /// 用于消除「Ended 消息已入队但 consumer 尚未 drain」窗口内 ensure_follow
    /// 误判幂等、导致本次切 Tab 不建流的问题。
    pub fn follow_is_alive(&self) -> bool {
        self.follow_active.iter().any(|a| a.load(Ordering::Acquire))
    }

    /// 建流并递增 stream epoch；不推进 buffer epoch，续流时保留已缓冲日志与待应用文本。
    ///
    /// 异步运行时（退避重连、存活标志、结束通知）全部在 `follow_runtime` 模块，
    /// 本方法只负责写入状态字段。
    fn spawn_follow(&mut self, key: &str, targets: Vec<(String, String)>, tail: i64) {
        self.follow_id = key.to_string();
        self.stream_epoch = self.stream_epoch.wrapping_add(1);
        let epoch = self.stream_epoch;
        let handle = crate::follow_runtime::spawn_follow(
            &self.rt,
            targets.clone(),
            tail,
            epoch,
            self.tx.clone(),
        );
        self.follow_targets = targets;
        self.follow_cancel = Some(handle.cancel);
        self.follow_active = handle.active;
    }
}
