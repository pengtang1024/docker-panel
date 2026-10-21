//! 台账内联树回调（分组的展开 / 收起）
//!
//! 纪律（AGENTS.md 规范 #18）：分组的收起状态是**用户操作过的状态**——
//! 只由本文件（UI 回调，UI 线程）写入 `AppState`；consumer 只读
//! （3s 刷新时按当前展开状态重建树），绝不回写。
//!
//! 展开 / 收起会改变**可见容器集合** → 台账行、概览带、筛选栏芯片计数
//! 三者必须同批重建（它们的口径都是「可见集合」）。

use std::sync::atomic::Ordering;

use slint::{ComponentHandle, SharedString, Weak};

use crate::slint_gen::{AppActions, MainWindow};
use crate::ui_state::SharedState;

/// 绑定内联树回调
pub fn bind(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    let ui_w: Weak<MainWindow> = _ui.as_weak();
    let s = state.clone();
    // 分组键：`project:<项目名>` / `svc:<项目名>/<服务名>`
    actions.on_compose_toggle(move |key: SharedString| {
        let Some(ui) = ui_w.upgrade() else { return };
        let (rows, summary, visible) = {
            let Ok(mut st) = s.lock() else { return };
            let key = key.to_string();
            // 存的是「收起」的分组键：已在集合里 = 收起 → 移除；否则加入
            if st.compose_collapsed.contains(&key) {
                st.compose_collapsed.remove(&key);
            } else {
                st.compose_collapsed.insert(key);
            }
            // 行模型作废：consumer 手里那批行是旧展开状态建的
            st.rows_epoch.fetch_add(1, Ordering::AcqRel);
            st.persist();
            crate::ui_rows::build_container_ledger(&st)
        };
        ui.set_container_rows(crate::render::containers(rows));
        ui.set_ledger_summary(summary);
        ui.set_chip_total(visible.0 as i32);
        ui.set_chip_running(visible.1 as i32);
    });
}
