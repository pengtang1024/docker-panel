//! 网络 / 卷操作回调（AppActions global）
//!
//! 网络与卷没有定时刷新，选中后必须**立即**重建抽屉内容（不等下一次 Msg），
//! 否则抽屉会停留在上一次选中的资源上。

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::{SelKind, SharedState};

pub fn bind_netvol_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();

    // 网络卡片点击 → 打开抽屉
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_network_select(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            open_selected(&ui, &s, SelKind::Network, id);
        });
    }

    // 卷卡片点击 → 打开抽屉
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_volume_select(move |row_name: SharedString| {
            let name = row_name.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            open_selected(&ui, &s, SelKind::Volume, name);
        });
    }

    // 删除网络
    {
        let s = state.clone();
        actions.on_network_remove(move |row_id: SharedString| {
            let id = row_id.to_string();
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let result = docker::remove_network(&id).await;
                let _ = tx.send(Msg::ActionDone("Network removed".into(), result));
            });
        });
    }

    // 删除卷
    {
        let s = state.clone();
        actions.on_volume_remove(move |row_name: SharedString| {
            let name = row_name.to_string();
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let result = docker::remove_volume(&name).await;
                let _ = tx.send(Msg::ActionDone("Volume removed".into(), result));
            });
        });
    }
}

/// 写 sel/sel_key → 立即按当前 AppState 重建抽屉内容并灌入 UI
fn open_selected(ui: &MainWindow, s: &SharedState, kind: SelKind, key: String) {
    let data = {
        let Ok(mut st) = s.lock() else { return };
        st.sel = kind;
        st.sel_key = key;
        st.clear_logs();
        crate::detail_fmt::build_drawer_data(&st)
    };
    ui.set_drawer_open(true);
    ui.set_drawer_tab(0);
    if let Some(d) = data {
        crate::ui_apply::apply_drawer(ui, &d);
    }
}
