//! 资源列表排序回调（镜像 / 网络 / 卷的**表头点击**）
//!
//! 纪律（AGENTS.md 规范 #18）：排序状态是**用户操作过的状态**——只由本文件
//! （UI 回调，UI 线程）写入 `AppState`；consumer 只读（3s 刷新时按当前排序重建行），
//! 绝不回写（回写会把表头方向打回旧值）。
//!
//! 三个视图的排序**互相独立**（各存各的），切视图回来保留。

use std::sync::atomic::Ordering;

use slint::{ComponentHandle, Weak};

use crate::slint_gen::{AppActions, MainWindow};
use crate::ui_state::SharedState;

/// 绑定三个资源列表的排序回调
pub fn bind(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    bind_images(ui, actions, state);
    bind_networks(ui, actions, state);
    bind_volumes(ui, actions, state);
}

/// 镜像列表排序（0=名称 1=大小 2=创建时间）
fn bind_images(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    let ui_w: Weak<MainWindow> = ui.as_weak();
    let s = state.clone();
    actions.on_sort_images(move |f: i32| {
        let Some(ui) = ui_w.upgrade() else { return };
        let packed = {
            let Ok(mut st) = s.lock() else { return };
            st.sort_images.pick(f.clamp(0, 2) as u8);
            st.rows_epoch.fetch_add(1, Ordering::AcqRel);
            st.persist();
            st.sort_images.to_ui()
        };
        ui.set_sort_images(packed);
        if let Ok(st) = s.lock() {
            crate::ui_apply::refresh_lists_now(&ui, &st);
        }
    });
}

/// 网络列表排序（0=名称 1=driver 2=子网）
fn bind_networks(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    let ui_w: Weak<MainWindow> = ui.as_weak();
    let s = state.clone();
    actions.on_sort_networks(move |f: i32| {
        let Some(ui) = ui_w.upgrade() else { return };
        let packed = {
            let Ok(mut st) = s.lock() else { return };
            st.sort_networks.pick(f.clamp(0, 2) as u8);
            st.rows_epoch.fetch_add(1, Ordering::AcqRel);
            st.persist();
            st.sort_networks.to_ui()
        };
        ui.set_sort_networks(packed);
        if let Ok(st) = s.lock() {
            crate::ui_apply::refresh_lists_now(&ui, &st);
        }
    });
}

/// 卷列表排序（0=名称 1=大小 2=挂载点）
fn bind_volumes(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    let ui_w: Weak<MainWindow> = ui.as_weak();
    let s = state.clone();
    actions.on_sort_volumes(move |f: i32| {
        let Some(ui) = ui_w.upgrade() else { return };
        let packed = {
            let Ok(mut st) = s.lock() else { return };
            st.sort_volumes.pick(f.clamp(0, 2) as u8);
            st.rows_epoch.fetch_add(1, Ordering::AcqRel);
            st.persist();
            st.sort_volumes.to_ui()
        };
        ui.set_sort_volumes(packed);
        if let Ok(st) = s.lock() {
            crate::ui_apply::refresh_lists_now(&ui, &st);
        }
    });
}
