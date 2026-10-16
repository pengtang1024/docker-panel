//! UI 回调绑定：所有 Slint UI 事件 → Docker 操作的桥接
//!
//! 拆分为三个文件：
//! - callbacks.rs       ：顶栏按钮 + Pull/Cleanup 弹窗
//! - callbacks_container.rs ：容器启停/重启/删除/日志
//! - callbacks_image.rs     ：镜像删除

pub use super::callbacks_container::bind_container_actions;
pub use super::callbacks_image::bind_image_actions;

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_callbacks(ui: &MainWindow, state: &SharedState) {
    bind_top_bar(ui, state);
    bind_pull_dialog(ui, state);
    bind_cleanup_dialog(ui, state);
    bind_container_actions(ui, state);
    bind_image_actions(ui, state);
}

fn bind_top_bar(ui: &MainWindow, state: &SharedState) {
    // 手动刷新
    {
        let ui_r = ui.as_weak();
        let s = state.clone();
        ui.on_refresh_requested(move || {
            let Some(ui) = ui_r.upgrade() else { return };
            ui.set_status_text(SharedString::from("手动刷新中..."));
            if let Ok(mut st) = s.lock() {
                st.refresh();
            }
        });
    }

    // 搜索
    {
        let s = state.clone();
        ui.on_search_accepted(move |text| {
            if let Ok(mut st) = s.lock() {
                let q = text.to_string();
                st.container_query = q.clone();
                st.image_query = q;
            }
        });
    }

    // 停止全部运行中容器
    {
        let ui_s = ui.as_weak();
        let s = state.clone();
        ui.on_stop_all_requested(move || {
            let Some(ui) = ui_s.upgrade() else { return };
            ui.set_status_text(SharedString::from("正在停止全部..."));
            let (rt_handle, ids, tx) = {
                let Ok(st) = s.lock() else { return };
                let ids: Vec<String> = st
                    .containers
                    .iter()
                    .filter(|c| c.is_running())
                    .map(|c| c.id.clone())
                    .collect();
                (st.rt.handle().clone(), ids, st.tx.clone())
            };
            if ids.is_empty() {
                return;
            }
            rt_handle.spawn(async move {
                let mut ok = 0u32;
                let mut fail = 0u32;
                for id in ids {
                    match docker::stop_container(&id).await {
                        Ok(_) => ok += 1,
                        Err(_) => fail += 1,
                    }
                }
                let desc = format!("停止 {ok} 个容器，失败 {fail} 个");
                let _ = tx.send(Msg::ActionDone(desc, Ok("ok".into())));
            });
        });
    }
}

fn bind_pull_dialog(ui: &MainWindow, state: &SharedState) {
    {
        let ui_p = ui.as_weak();
        ui.on_pull_requested(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(true);
            ui.set_pull_input(SharedString::from(""));
            ui.set_pull_busy(false);
        });
    }
    {
        let ui_p = ui.as_weak();
        ui.on_pull_close(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(false);
        });
    }
    {
        let ui_p = ui.as_weak();
        let s = state.clone();
        ui.on_pull_submit(move |spec: SharedString| {
            let Some(ui) = ui_p.upgrade() else { return };
            let text = spec.to_string();
            if text.trim().is_empty() {
                ui.set_status_text(SharedString::from("请输入镜像名，例如 nginx:latest"));
                return;
            }
            ui.set_pull_busy(true);
            ui.set_status_text(SharedString::from(format!("开始拉取 {text}...")));

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

fn bind_cleanup_dialog(ui: &MainWindow, state: &SharedState) {
    {
        let ui_c = ui.as_weak();
        ui.on_cleanup_requested(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(true);
        });
    }
    {
        let ui_c = ui.as_weak();
        ui.on_cleanup_close(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(false);
        });
    }
    {
        let ui_c = ui.as_weak();
        let s = state.clone();
        ui.on_cleanup_submit(move |stopped: bool, dangling: bool, unused: bool| {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(false);
            if !stopped && !dangling && !unused {
                ui.set_status_text(SharedString::from("请至少勾选一项"));
                return;
            }
            ui.set_status_text(SharedString::from("执行清理..."));
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let result = docker::cleanup(stopped, dangling, unused).await;
                let _ = tx.send(Msg::CleanupDone(result));
            });
        });
    }
}
