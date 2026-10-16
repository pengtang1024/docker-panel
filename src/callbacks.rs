//! UI 回调绑定：所有 Slint UI 事件 → Docker 操作的桥接
//!
//! 统一的回调套路（所有 on_xxx_requested 闭包都一样）：
//!
//!     // 1. 主线程 UI 回调触发（快进快出，不能阻塞事件循环）
//!     {
//!         let (rt_handle, tx) = {
//!             let Ok(st) = s.lock() else { return };   // 锁内：取 handle + tx clone
//!             (st.rt.handle().clone(), st.tx.clone())
//!         };                                           // ← 立即 drop Mutex
//!
//!         // 2. 锁外 spawn 异步任务做真正的 Docker 操作
//!         rt_handle.spawn(async move {
//!             let result = docker::do_thing().await;
//!             let _ = tx.send(Msg::ActionDone(..., result));  // 结果送回 channel
//!         });
//!     }
//!     // 3. consumer 线程稍后收到 Msg → invoke_from_event_loop 更新 Slint Model
//!
//! 为什么"锁内只取 handle，锁外 spawn"：
//!   Mutex 的目的是保护容器/镜像列表、query 等可共享数据。
//!   锁内取 tx 和 rt handle 只是 clone Arc/Sender，微秒级。
//!   如果在锁内 spawn 就立刻做 Docker API 调用——bollard 本身是 async
//!   但 spawn 闭包里的 await 不会阻塞 spawn 那一刻的调用栈，
//!   不过真正要小心的是：**锁内不能做任何可能长时间占着 Mutex 的事**。
//!   规范简化：任何涉及"spawn 后要等结果"的，都先 drop 锁再 spawn。
//!
//! 为什么不直接在 UI 回调里改 Slint Model：
//!   技术上主线程改自己的 Model 完全合法。
//!   但如果以后某个 UI 回调需要等一个 async 结果再决定改什么，
//!   直接写就会阻塞 Slint 事件循环。统一走 channel → consumer，
//!   每个操作都变成"spawn + 等回调"的一致模型。

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
            // —— 两段式锁：锁内只收集 id 列表 + clone handle/tx → 立即 drop ——
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
            // 锁外 spawn 真正的批量操作
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
    let actions = ui.global::<crate::slint_gen::AppActions>();

    // 容器启停切换
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_container_toggle(move |row_id: SharedString| {
            let id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("切换容器状态..."));

            // 锁内判断 is_running + clone handle/tx → drop
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
            // 锁外 spawn
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

    // 容器 inspect（点详情按钮或点容器行时调用）
    let bind_container_inspect = |s: &SharedState, id: String| {
        if let Ok(mut st) = s.lock() {
            st.request_detail(id);
            st.clear_logs();
        }
    };

    // 容器日志按钮 → inspect + follow_logs（锁外 spawn）
    {
        let s = state.clone();
        actions.on_container_logs(move |row_id: SharedString| {
            let id = row_id.to_string();
            bind_container_inspect(&s, id.clone());
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
