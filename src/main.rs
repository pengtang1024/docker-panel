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

mod callbacks;
mod docker;
mod models;
mod slint_gen;
mod ui_state;

use slint_gen::*;

use std::sync::mpsc::{Receiver, RecvError};

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
    let pump_and_apply = move |state: &SharedState, rx: &Receiver<Msg>, first: Option<Msg>| {
        let Ok(mut s) = state.lock() else { return };
        let (need_update, latest_msg) = s.drain_messages(rx, first);
        eprintln!("[pump] need_update={need_update} latest={:?}", latest_msg.as_ref().map(|m| format!("{:?}", std::mem::discriminant(m))));
        if !need_update { return; }

        // 锁内：构建 rows + 读字段
        let container_rows = build_container_rows(&s);
        let image_rows = build_image_rows(&s);
        eprintln!("[pump] built rows: containers={} images={}", container_rows.len(), image_rows.len());
        let running = s.containers.iter().filter(|c| c.is_running()).count();
        let running_total = format!("{}/{}", running, s.containers.len());
        let image_total = s.images.len().to_string();
        let containers_len = s.containers.len();
        let update = derive_update(containers_len, &latest_msg);
        drop(s); // ⚠️ 关键：必须在 invoke_from_event_loop 之前释放锁

        let ui_w = ui_w_pump.clone();
        let inv = invoke_from_event_loop(move || {
            let Some(ui) = ui_w.upgrade() else { eprintln!("[pump] upgrade failed!"); return };
            eprintln!("[pump] invoke UI set: containers={} images={} docker_ok={}", 
                container_rows.len(), image_rows.len(), update.docker_ok);
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
        });
        if let Err(e) = inv {
            eprintln!("[pump] invoke_from_event_loop failed: {e}");
        }
    };

    // —— consumer 线程：独占 rx，**锁外** 阻塞 recv ——
    {
        let state_c = state.clone();
        let rx_c = rx; // 独占所有权，从此再也不进 Mutex
        let pump_c = pump_and_apply.clone();
        std::thread::spawn(move || {
            let mut first = true;
            loop {
                // 先扫一次积压（非阻塞，无 first msg）
                if first {
                    eprintln!("[consumer] 启动，初始 drain");
                    first = false;
                }
                pump_c(&state_c, &rx_c, None);

                // ⚠️ 锁外阻塞！主线程拿 state 锁永远不被这条 recv 卡住
                match rx_c.recv() {
                    Ok(msg) => {
                        eprintln!("[consumer] 收到一条消息: {:?}", std::mem::discriminant(&msg));
                        // 关键：把 recv() 消费走的 msg 传给 pump_c 处理！
                        pump_c(&state_c, &rx_c, Some(msg));
                    }
                    Err(RecvError) => {
                        eprintln!("[consumer] channel 关闭，退出");
                        break;
                    }
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
        let Ok(s) = state.lock() else {
            return;
        };
        let tx = s.tx.clone();
        let rt = s.rt.handle().clone();
        drop(s);
        rt.spawn(async move {
            let _ = connect();
            eprintln!("[init] connect done");
            let c_res = list_containers(true).await;
            eprintln!("[init] list_containers done: is_ok={} len={}", c_res.is_ok(), c_res.as_ref().map(|l| l.len()).unwrap_or(0));
            let i_res = list_images().await;
            eprintln!("[init] list_images done: is_ok={} len={}", i_res.is_ok(), i_res.as_ref().map(|l| l.len()).unwrap_or(0));
            let sc = tx.send(Msg::Containers(c_res));
            eprintln!("[init] send Containers: {:?}", sc.err());
            let si = tx.send(Msg::Images(i_res));
            eprintln!("[init] send Images: {:?}", si.err());
        });
    }

    // —— 回调绑定 ——
    callbacks::bind_callbacks(&ui, &state);

    // —— 进入 Slint 主循环 ——
    let _ = ui.run();
}
