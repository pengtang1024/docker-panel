//! 容器操作回调（AppActions global）

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_container_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();

    // 容器启停切换
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_toggle(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("切换容器状态..."));

            let is_running = {
                let Ok(st) = s.lock() else { return };
                st.containers
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.is_running())
                    .unwrap_or(false)
            };
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            if is_running {
                rt_handle.spawn(async move {
                    match docker::stop_container(&id).await {
                        Ok(_) => {
                            let _ = tx.send(Msg::ActionDone("容器已停止".into(), Ok("ok".into())));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::ActionDone("停止失败".into(), Err(e)));
                        }
                    }
                });
            } else {
                rt_handle.spawn(async move {
                    match docker::start_container(&id).await {
                        Ok(_) => {
                            let _ = tx.send(Msg::ActionDone("容器已启动".into(), Ok("ok".into())));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::ActionDone("启动失败".into(), Err(e)));
                        }
                    }
                });
            }
        });
    }

    // 容器重启
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_restart(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("重启容器..."));
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::restart_container(&id).await {
                    Ok(_) => {
                        let _ = tx.send(Msg::ActionDone("容器已重启".into(), Ok("ok".into())));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("重启失败".into(), Err(e)));
                    }
                }
            });
        });
    }

    // 容器删除
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_remove(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("删除容器..."));
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::remove_container(&id).await {
                    Ok(_) => {
                        let _ = tx.send(Msg::ActionDone("容器已删除".into(), Ok("ok".into())));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("删除失败".into(), Err(e)));
                    }
                }
            });
        });
    }

    // 容器 inspect（锁内）
    let bind_inspect = |s: &SharedState, id: String| {
        if let Ok(mut st) = s.lock() {
            st.request_detail(id);
            st.clear_logs();
        }
    };

    // 容器日志按钮 → inspect + follow_logs
    {
        let s = state.clone();
        actions.on_container_logs(move |row_id: SharedString| {
            let id = row_id.to_string();
            bind_inspect(&s, id.clone());
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let _ = docker::follow_logs(&id, tx, 200).await;
            });
        });
    }
}
