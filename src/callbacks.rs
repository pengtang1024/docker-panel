//! UI 回调绑定：所有 Slint UI 事件 → Docker 操作的桥接
//!
//! 每个回调闭包遵循统一模式：
//! 1. ui.upgrade() 拿 UI 句柄
//! 2. 从 state 取 rt handle + tx
//! 3. spawn 异步任务 → 调 docker API → tx.send(Msg::ActionDone 等)
//! 4. consumer 线程收到 Msg → invoke_from_event_loop 更新 Slint Model

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_callbacks(ui: &MainWindow, state: &SharedState) {
    // —— 顶栏按钮 ——

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
                let mut ok = 0;
                let mut fail = 0;
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

    // —— Pull 弹窗 ——
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

    // —— Cleanup 弹窗 ——
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

    // —— AppActions global（delegate 按钮触发）——
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
                        Ok(_) => { let _ = tx.send(Msg::ActionDone("容器已停止".into(), Ok("ok".into()))); }
                        Err(e) => { let _ = tx.send(Msg::ActionDone("停止失败".into(), Err(e))); }
                    }
                });
            } else {
                rt_handle.spawn(async move {
                    match docker::start_container(&id).await {
                        Ok(_) => { let _ = tx.send(Msg::ActionDone("容器已启动".into(), Ok("ok".into()))); }
                        Err(e) => { let _ = tx.send(Msg::ActionDone("启动失败".into(), Err(e))); }
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
                    Ok(_) => { let _ = tx.send(Msg::ActionDone("容器已重启".into(), Ok("ok".into()))); }
                    Err(e) => { let _ = tx.send(Msg::ActionDone("重启失败".into(), Err(e))); }
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
                    Ok(_) => { let _ = tx.send(Msg::ActionDone("容器已删除".into(), Ok("ok".into()))); }
                    Err(e) => { let _ = tx.send(Msg::ActionDone("删除失败".into(), Err(e))); }
                }
            });
        });
    }

    // 镜像删除
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_image_remove(move |row_id: SharedString| {
            let full_id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("删除镜像..."));

            let (name, tag) = match full_id.rsplit_once(':') {
                Some((n, t)) if !n.is_empty() && !t.contains('/') => (n.to_string(), t.to_string()),
                _ => (full_id, "".to_string()),
            };
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::remove_image(&name, &tag).await {
                    Ok(_) => { let _ = tx.send(Msg::ActionDone("镜像已删除".into(), Ok("ok".into()))); }
                    Err(e) => { let _ = tx.send(Msg::ActionDone("删除镜像失败".into(), Err(e))); }
                }
            });
        });
    }
}
