//! `/system/df` 两路采样（卷占用 / 容器可写层）的消息应用
//!
//! 容器「磁盘」列 = 可写层 + 挂载卷，两路采样互相独立：任一路到达都要重算。
//! 抽屉详情里的「磁盘」行读的也是 `size_rw`，所以选中容器时同样要置
//! `drawer_changed`——否则已停止容器（没有 stats 推送）的抽屉磁盘会一直停在
//! 打开时的旧值。

use std::collections::HashMap;

use crate::ui_state::{AppState, SelKind};
use crate::ui_update::{DrainResult, UiFlag};

/// 两路 df 采样共用的变化标记（口径与 `apply_container_sizes` 一致）：
/// 没有容器列表时不标记——首屏 df 常早于列表到达，置标记只会白占一个 UI 窗口。
fn mark_df_changes(state: &AppState, res: &mut DrainResult) {
    if state.containers.is_empty() {
        return;
    }
    res.mark(UiFlag::ContainersChanged);
    if state.sel == SelKind::Container {
        res.mark(UiFlag::DrawerChanged);
    }
}

impl AppState {
    /// 卷占用到达：卷列 + 容器磁盘（含挂载卷）都要重算
    pub(super) fn apply_volume_sizes_msg(
        &mut self,
        map: HashMap<String, u64>,
        res: &mut DrainResult,
    ) {
        if !self.volumes.is_empty() {
            res.mark(UiFlag::VolumesChanged);
        }
        mark_df_changes(self, res);
        self.volume_sizes = map;
        self.apply_container_sizes();
    }

    /// 容器可写层占用到达
    pub(super) fn apply_container_sizes_msg(
        &mut self,
        map: HashMap<String, u64>,
        res: &mut DrainResult,
    ) {
        mark_df_changes(self, res);
        self.container_sizes = map;
        self.apply_container_sizes();
    }
}
