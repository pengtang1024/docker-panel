//! UI 回调绑定

pub use super::callbacks_container::bind_container_actions;
pub use super::callbacks_image::bind_image_actions;

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_callbacks(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();
    bind_top_bar(ui, &actions, state);
    bind_sub_bar(ui, &actions, state);
    bind_pull_dialog(ui, &actions, state);
    bind_cleanup_dialog(ui, &actions, state);
    bind_drawer(ui, &actions, state);
    bind_container_actions(ui, state);
    bind_image_actions(ui, state);
}

fn bind_top_bar(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let s = state.clone();
        actions.on_refresh(move || {
            if let Ok(mut st) = s.lock() {
                st.refresh();
            }
        });
    }
    {
        let s = state.clone();
        actions.on_search(move |text: SharedString| {
            let q = text.to_string();
            if let Ok(mut st) = s.lock() {
                st.container_query = q.clone();
                st.image_query = q;
            }
        });
    }
    {
        let ui_p = ui.as_weak();
        actions.on_pull(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(true);
            ui.set_pull_input(SharedString::from(""));
            ui.set_pull_busy(false);
        });
    }
    {
        let ui_c = ui.as_weak();
        actions.on_cleanup(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(true);
        });
    }
    {
        let ui_t = ui.as_weak();
        actions.on_toggle_theme(move || {
            let Some(ui) = ui_t.upgrade() else { return };
            let cur = ui.get_theme_mode();
            let next = if cur >= 2 { 0 } else { cur + 1 };
            ui.set_theme_mode(next);
        });
    }
}

fn bind_sub_bar(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        actions.on_set_view(move |_v: i32| {});
    }
    {
        let s = state.clone();
        actions.on_set_filter(move |f: SharedString| {
            let key = f.to_string();
            if let Ok(mut st) = s.lock() {
                st.filter = key;
            }
        });
    }
    {
        actions.on_set_sort(move |_f: i32| {});
    }
}

fn bind_pull_dialog(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_p = _ui.as_weak();
        actions.on_pull_close(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(false);
        });
    }
    {
        let ui_p = _ui.as_weak();
        let s = state.clone();
        actions.on_pull_submit(move |spec: SharedString| {
            let Some(ui) = ui_p.upgrade() else { return };
            let text = spec.to_string();
            if text.trim().is_empty() {
                ui.set_status_text(SharedString::from("请输入镜像名"));
                return;
            }
            ui.set_pull_busy(true);
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let tx_clone = tx.clone();
                let result = docker::pull_image(&text, move |progress| {
                    let _ = tx_clone.send(Msg::PullProgress(progress));
                })
                .await;
                let _ = tx.send(Msg::PullDone(result));
            });
        });
    }
}

fn bind_cleanup_dialog(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_c = _ui.as_weak();
        actions.on_cleanup_close(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(false);
        });
    }
    {
        let ui_c = _ui.as_weak();
        let s = state.clone();
        actions.on_cleanup_submit(move |stopped: bool, dangling: bool, unused_net: bool, unused_vol: bool| {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(false);
            if !stopped && !dangling && !unused_net && !unused_vol {
                ui.set_status_text(SharedString::from("请至少勾选一项"));
                return;
            }
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let result = docker::cleanup(stopped, dangling, unused_net).await;
                let _ = tx.send(Msg::CleanupDone(result));
            });
        });
    }
}

fn bind_drawer(ui: &MainWindow, actions: &AppActions, _state: &SharedState) {
    {
        let ui_d = ui.as_weak();
        actions.on_drawer_close(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            ui.set_drawer_open(false);
        });
    }
    {
        let ui_d = ui.as_weak();
        actions.on_drawer_tab(move |t: i32| {
            let Some(ui) = ui_d.upgrade() else { return };
            ui.set_drawer_tab(t);
        });
    }
    {
        let ui_d = ui.as_weak();
        actions.on_log_toggle_follow(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            ui.set_log_follow(!ui.get_log_follow());
        });
    }
    {
        actions.on_log_copy(move || {});
        actions.on_log_clear(move || {});
    }
}
