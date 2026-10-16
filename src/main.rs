//! docker-panel — 极简 Docker 桌面管理工具
//!
//! Slint 原生 GUI + 软件渲染器，零 GPU 依赖。
//!
//! 线程模型（修死锁版）：
//! ┌──────────────┐  ui.run() 阻塞  ┌──────────────────┐
//! │  主线程       │──Timer/回调──▶│  Mutex<AppState>  │
//! └──────────────┘                 └────────┬─────────┘
//!                                            │ tx.send()
//! ┌──────────────┐  rx.recv() 阻塞（锁外！） ┌▼──────────────────┐
//! │ consumer 线程│◀─────────────────────────│  mpsc::Receiver   │
//! └──────────────┘                          └───────────────────┘
//!   recv 到 → lock state → drain_messages → drop lock → invoke_from_event_loop 更新 Slint
//!   （锁持有时间 = drain + build rows，微秒级）

mod docker;
mod models;
mod ui_state;

slint::include_modules!();

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvError};

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, invoke_from_event_loop};

use crate::docker::{connect, list_containers, list_images};
use crate::models::Msg;
use crate::ui_state::{
    AppState, SharedState, derive_update, format_bytes, REFRESH_INTERVAL,
};

fn main() {
    // —— 创建：state + rx 分离，rx 交给 consumer 独占 ——
    let (state, rx): (SharedState, Receiver<Msg>) = AppState::new();

    let ui = MainWindow::new().expect("create window");
    let ui_weak = ui.as_weak();

    // —— 构建 Slint rows（锁内构建，锁外应用）——
    let build_container_rows = |s: &AppState| -> Vec<ContainerRow> {
        s.containers
            .iter()
            .filter(|c| {
                if s.container_query.is_empty() {
                    true
                } else {
                    let q = s.container_query.to_lowercase();
                    c.name.to_lowercase().contains(&q)
                        || c.short_id().contains(&q)
                        || c.image.to_lowercase().contains(&q)
                        || c.state.to_lowercase().contains(&q)
                }
            })
            .map(|c| ContainerRow {
                id: SharedString::from(c.id.clone()),
                short_id: SharedString::from(c.short_id()),
                name: SharedString::from(c.name.clone()),
                image: SharedString::from(c.image.clone()),
                status: SharedString::from(c.status.clone()),
                is_running: c.is_running(),
            })
            .collect()
    };
    let build_image_rows = |s: &AppState| -> Vec<ImageRow> {
        s.images
            .iter()
            .filter(|img| {
                if s.image_query.is_empty() {
                    true
                } else {
                    let q = s.image_query.to_lowercase();
                    img.name.to_lowercase().contains(&q)
                        || img.tag.to_lowercase().contains(&q)
                        || img.short_id().contains(&q)
                }
            })
            .map(|img| ImageRow {
                id: SharedString::from(img.key()),
                short_id: SharedString::from(img.short_id()),
                name: SharedString::from(img.name.clone()),
                tag: SharedString::from(img.tag.clone()),
                size: SharedString::from(format_bytes(img.size as u64)),
                containers: img.containers as i32,
            })
            .collect()
    };

    // —— UI 更新泵 ——
    let ui_w_pump = ui_weak.clone();
    let pump_and_apply = move |state: &SharedState, rx: &Receiver<Msg>| {
        let Ok(mut s) = state.lock() else { return };
        let (need_update, latest_msg) = s.drain_messages(rx);
        if !need_update { return; }

        // 锁内：构建 rows + 读字段
        let container_rows = build_container_rows(&s);
        let image_rows = build_image_rows(&s);
        let running = s.containers.iter().filter(|c| c.is_running()).count();
        let running_total = format!("{}/{}", running, s.containers.len());
        let image_total = s.images.len().to_string();
        let containers_len = s.containers.len();
        let update = derive_update(containers_len, &latest_msg);
        drop(s); // ⚠️ 关键：必须在 invoke_from_event_loop 之前释放锁

        let ui_w = ui_w_pump.clone();
        invoke_from_event_loop(move || {
            let Some(ui) = ui_w.upgrade() else { return };
            let cm: ModelRc<ContainerRow> = ModelRc::from(container_rows.as_slice());
            let im: ModelRc<ImageRow> = ModelRc::from(image_rows.as_slice());
            ui.set_container_rows(cm);
            ui.set_image_rows(im);
            ui.set_container_count(SharedString::from(running_total));
            ui.set_image_count(SharedString::from(image_total));
            ui.set_docker_ok(update.docker_ok);
            ui.set_status_text(SharedString::from(update.status_text));
            if update.reset_pull_busy { ui.set_pull_busy(false); }
            if update.close_pull { ui.set_pull_open(false); }
        }).ok();
    };

    // —— consumer 线程：独占 rx，**锁外** 阻塞 recv ——
    {
        let state_c = state.clone();
        let rx_c = rx; // 独占所有权，从此再也不进 Mutex
        let pump_c = pump_and_apply.clone();
        std::thread::spawn(move || loop {
            // 先扫一次积压（非阻塞）
            let _ = pump_c(&state_c, &rx_c);

            // ⚠️ 锁外阻塞！主线程拿 state 锁永远不被这条 recv 卡住
            match rx_c.recv() {
                Ok(_msg) => {
                    pump_c(&state_c, &rx_c);
                }
                Err(RecvError) => {
                    break; // channel 关闭
                }
            }
        });
    }

    // —— 定时刷新（主线程 Timer，3s）——
    {
        let state_t = state.clone();
        let pump_t = pump_and_apply.clone();
        Timer::default().start(TimerMode::Repeated, REFRESH_INTERVAL, move || {
            // 主线程触发 refresh → tokio 异步发 Msg → consumer 收
            if let Ok(mut s) = state_t.lock() {
                s.refresh();
            }
            // 让 consumer 线程立刻 drain（不用等下一条 recv）
            // 这里 rx 不在 state 里了，传一个 dummy receiver 给 drain 没用
            // 但 refresh() 已经 spawn 了异步任务发送 Msg
            // consumer 线程正在 recv() 阻塞，收到新 Msg 会被唤醒并 pump
            // 所以这里直接 return，等 consumer 处理
            let _ = pump_t; // 让 pump_t 活过闭包作用域
        });
    }

    // —— 初始 Docker 连接 + 列表加载 ——
    {
        let Ok(mut s) = state.lock() else {
            return;
        };
        let tx = s.tx.clone();
        let rt = s.rt.handle().clone();
        drop(s);
        eprintln!("[docker-panel] 初始任务提交到 tokio...");
        rt.spawn(async move {
            eprintln!("[docker-panel] tokio 任务开始执行");
            let _ = connect();
            eprintln!("[docker-panel] connect() 完成");
            let c_res = list_containers(true).await;
            eprintln!("[docker-panel] list_containers: {}", c_res.as_ref().map(|l| l.len()).unwrap_or(0));
            let i_res = list_images().await;
            eprintln!("[docker-panel] list_images: {}", i_res.as_ref().map(|l| l.len()).unwrap_or(0));
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
            eprintln!("[docker-panel] 初始数据已 send → consumer rx");
        });
    }

    // —— 回调绑定 ——
    bind_callbacks(&ui, &state);

    // —— 进入 Slint 主循环 ——
    let _ = ui.run();
}

fn bind_callbacks(ui: &MainWindow, state: &SharedState) {
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

    // 搜索（立即应用：直接 pump，但 pump 需要 rx —— 这里简化，存 query 就够了，下次 consumer 触发会重算）
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

    // 停止全部
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

    // Pull 弹窗
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

    // Cleanup 弹窗
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
