//! 容器操作回调（AppActions global）

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::{Msg, SortField};
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_container_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();

    // —— 表头点击排序 ——
    {
        let s = state.clone();
        actions.on_container_sort(move |field: i32| {
            let sf = match field {
                0 => SortField::Group,
                1 => SortField::Name,
                2 => SortField::Status,
                3 => SortField::Cpu,
                4 => SortField::Mem,
                _ => return,
            };
            if let Ok(mut st) = s.lock() {
                if st.sort_field == sf {
                    st.sort_desc = !st.sort_desc;
                } else {
                    st.sort_field = sf;
                    st.sort_desc = false;
                }
            }
        });
    }

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

    // —— 整行点击 → inspect + 切详情面板 ——
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_select(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            // Rust 侧直接切面板 —— 行点击必然看详情
            ui.set_active_view(0);
            ui.set_status_text(SharedString::from(format!("查看 {id}")));
            bind_inspect(&s, id);
        });
    }

    // —— 日志按钮 → inspect + follow_logs + 切日志面板 ——
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_logs(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_active_view(1); // 🆕 自动切到日志面板
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

    // —— Checkbox 切换选中 ——
    {
        let s = state.clone();
        actions.on_container_toggle_select(move |row_id: SharedString| {
            if let Ok(mut st) = s.lock() {
                st.batch.toggle(row_id.to_string());
            }
        });
    }

    // —— 批量清除选中 ——
    {
        let s = state.clone();
        actions.on_batch_clear(move || {
            if let Ok(mut st) = s.lock() {
                st.batch.clear();
            }
        });
    }

    // —— 批量启动 ——
    {
        let s = state.clone();
        actions.on_batch_start(move || {
            let ids = {
                let Ok(st) = s.lock() else { return };
                st.batch.ids_sorted()
            };
            for id in ids {
                spawn_container_op(&s, &id, OpKind::Start);
            }
        });
    }

    // —— 批量停止 ——
    {
        let s = state.clone();
        actions.on_batch_stop(move || {
            let ids = {
                let Ok(st) = s.lock() else { return };
                st.batch.ids_sorted()
            };
            for id in ids {
                spawn_container_op(&s, &id, OpKind::Stop);
            }
        });
    }

    // —— 批量重启 ——
    {
        let s = state.clone();
        actions.on_batch_restart(move || {
            let ids = {
                let Ok(st) = s.lock() else { return };
                st.batch.ids_sorted()
            };
            for id in ids {
                spawn_container_op(&s, &id, OpKind::Restart);
            }
        });
    }

    // —— 批量删除 ——
    {
        let s = state.clone();
        actions.on_batch_remove(move || {
            let ids = {
                let Ok(st) = s.lock() else { return };
                st.batch.ids_sorted()
            };
            for id in ids {
                spawn_container_op(&s, &id, OpKind::Remove);
            }
        });
    }
}

enum OpKind {
    Start,
    Stop,
    Restart,
    Remove,
}

fn spawn_container_op(s: &SharedState, id: &str, op: OpKind) {
    let (rt_handle, tx) = {
        let Ok(st) = s.lock() else { return };
        (st.rt.handle().clone(), st.tx.clone())
    };
    let id = id.to_string();
    rt_handle.spawn(async move {
        let (desc, result) = match op {
            OpKind::Start => (format!("启动 {id}"), docker::start_container(&id).await),
            OpKind::Stop => (format!("停止 {id}"), docker::stop_container(&id).await),
            OpKind::Restart => (format!("重启 {id}"), docker::restart_container(&id).await),
            OpKind::Remove => (format!("删除 {id}"), docker::remove_container(&id).await),
        };
        let _ = tx.send(Msg::ActionDone(desc, result));
    });
}
