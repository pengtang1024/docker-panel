//! 容器操作回调（AppActions global）

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::i18n::{t, trf};
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::{SelKind, SharedState};

pub fn bind_container_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();
    // 容器启停切换
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_toggle(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from(t("Toggling container...").as_str()));

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
                            let _ = tx
                                .send(Msg::ActionDone("Container stopped".into(), Ok("ok".into())));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::ActionDone("Failed to stop".into(), Err(e)));
                        }
                    }
                });
            } else {
                rt_handle.spawn(async move {
                    match docker::start_container(&id).await {
                        Ok(_) => {
                            let _ = tx
                                .send(Msg::ActionDone("Container started".into(), Ok("ok".into())));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::ActionDone("Failed to start".into(), Err(e)));
                        }
                    }
                });
            }
        });
    }

    // 容器重启
    {
        let s = state.clone();
        actions.on_container_restart(move |row_id: SharedString| {
            let id = row_id.to_string();
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::restart_container(&id).await {
                    Ok(_) => {
                        let _ = tx.send(Msg::ActionDone(
                            "Container restarted".into(),
                            Ok("ok".into()),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("Failed to restart".into(), Err(e)));
                    }
                }
            });
        });
    }

    // 容器删除
    {
        let s = state.clone();
        actions.on_container_remove(move |row_id: SharedString| {
            let id = row_id.to_string();
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::remove_container(&id).await {
                    Ok(_) => {
                        let _ =
                            tx.send(Msg::ActionDone("Container removed".into(), Ok("ok".into())));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("Failed to remove".into(), Err(e)));
                    }
                }
            });
        });
    }

    // 容器 inspect（锁内）：写 sel/sel_key，详情 Msg 到达后抽屉按容器渲染
    let bind_inspect = |s: &SharedState, id: String| {
        if let Ok(mut st) = s.lock() {
            st.sel = SelKind::Container;
            st.sel_key = id.clone();
            st.request_detail(id);
            st.clear_logs();
        }
    };

    // 卡片点击 → 打开抽屉 + inspect
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_select(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_drawer_open(true);
            ui.set_drawer_tab(0);
            bind_inspect(&s, id);
        });
    }

    // 日志按钮 → 打开抽屉日志 tab + follow_logs
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_logs(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_drawer_open(true);
            ui.set_drawer_tab(1);
            bind_inspect(&s, id.clone());
            if let Ok(mut st) = s.lock() {
                st.ensure_follow(&id);
            }
        });
    }

    // Checkbox 切换选中
    {
        let ui_c = ui.as_weak();
        let s = state.clone();
        actions.on_container_toggle_select(move |row_id: SharedString| {
            let Some(ui) = ui_c.upgrade() else { return };
            if let Ok(mut st) = s.lock() {
                st.batch.toggle(row_id.to_string());
                let n = st.batch.selected_ids.len() as i32;
                drop(st);
                ui.set_batch_count(n);
            }
        });
    }

    // 批量清除
    {
        let ui_b = ui.as_weak();
        let s = state.clone();
        actions.on_batch_clear(move || {
            if let Ok(mut st) = s.lock() {
                st.batch.clear();
                drop(st);
                if let Some(ui) = ui_b.upgrade() {
                    ui.set_batch_count(0);
                    ui.set_sel_mode(false);
                }
            }
        });
    }

    // 批量启动
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

    // 批量停止
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

    // 批量重启
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

    // 批量删除
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
            OpKind::Start => (trf("Start {0}", &[&id]), docker::start_container(&id).await),
            OpKind::Stop => (trf("Stop {0}", &[&id]), docker::stop_container(&id).await),
            OpKind::Restart => (
                trf("Restart {0}", &[&id]),
                docker::restart_container(&id).await,
            ),
            OpKind::Remove => (
                trf("Remove {0}", &[&id]),
                docker::remove_container(&id).await,
            ),
        };
        let _ = tx.send(Msg::ActionDone(desc, result));
    });
}
