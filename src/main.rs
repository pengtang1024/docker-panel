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
    // ═══════════════════════════════════════════════════════
    // Step 1: 建立状态 + channel
    //   AppState::new() 返回 (SharedState, Receiver<Msg>)
    //   — SharedState = Arc<Mutex<AppState>>   主线程、Timer 回调都碰它
    //   — Receiver<Msg>  交给 consumer 线程**独占**所有权，绝不进 Mutex
    //                      这是整个架构防死锁的关键：
    //                      consumer 的 rx.recv() 在锁外阻塞
    //                      主线程拿 state 锁永远不会被它卡住
    // ═══════════════════════════════════════════════════════
    let (state, rx): (SharedState, Receiver<Msg>) = AppState::new();

    let ui = MainWindow::new().expect("create window");
    let ui_weak = ui.as_weak();

    // —— 构建 Slint rows ——
    // 纯读取闭包：只消费 AppState，不持有锁、不做 IO
    // 每次 pump_and_apply 在**持有 state 锁期间**调用它们，
    // 构建出 Vec<ContainerRow> / Vec<ImageRow> 后 drop 锁，
    // 再由 invoke_from_event_loop 把这些数据塞进 Slint Model。
    //
    // 为什么分两阶段？
    //   Slint 的 Model 变更必须在主线程事件循环里（invoke_from_event_loop）。
    //   如果 pump_and_apply 全程持有 Mutex 等 invoke 消费完，
    //   主线程 Timer/回调在处理事件时若要再 lock state 就会卡住——
    //   因为 consumer 持有锁等主线程，主线程处理事件又要锁。
    //   → 经典"consumer 等主线程消费事件，主线程处理事件等 consumer 放锁"互锁。
    //   所以锁持有时间 = drain + build rows（微秒级），然后立即 drop。
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

    // ═══════════════════════════════════════════════════════
    // pump_and_apply — consumer 线程的"UI 更新泵"
    //
    // 职责：收一批 Msg → 更新 AppState → 构建 Slint rows →
    //       派发 invoke_from_event_loop 让主线程更新 Slint Model
    //
    // 时序约束（⚠️ 违反就死锁）：
    //   1. lock state    → drain_messages → build rows
    //   2. drop(state)   ←— 锁必须在 invoke_from_event_loop 之前释放
    //   3. invoke_from_event_loop(move || { ui.set_*()... })
    //
    // first 参数：consumer.recv() 消费掉的那一条 Msg 立刻交给 drain，
    // 因为 rx.try_recv() 再也看不到它了。
    // ═══════════════════════════════════════════════════════
    let ui_w_pump = ui_weak.clone();
    let pump_and_apply = move |state: &SharedState, rx: &Receiver<Msg>, first: Option<Msg>| {
        let Ok(mut s) = state.lock() else { return };
        let (need_update, latest_msg) = s.drain_messages(rx, first);
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

    // ═══════════════════════════════════════════════════════
    // Step 2: 启动 consumer 线程（架构中枢）
    //
    // 线程循环：
    //   loop {
    //     pump_c(&state_c, &rx_c, None);      // 先扫积压（锁外）
    //     match rx_c.recv() {                 // ⚠️ 锁外阻塞！
    //       Ok(msg) => pump_c(&state_c, &rx_c, Some(msg));
    //       Err(RecvError) => break;          // channel 关
    //     }
    //   }
    //
    // 所有权声明：rx_c 是 main 线程的 rx move 进来的，
    // 从此 main 再也碰不到它——consumer 独占。
    // 这样 consumer 持不持锁永远不影响主线程。
    // ═══════════════════════════════════════════════════════
    {
        let state_c = state.clone();
        let rx_c = rx; // 独占所有权，从此再也不进 Mutex
        let pump_c = pump_and_apply.clone();
        std::thread::spawn(move || {
            let mut first = true;
            loop {
                // 先扫一次积压（非阻塞，无 first msg）
                if first {
                    first = false;
                }
                pump_c(&state_c, &rx_c, None);

                // ⚠️ 锁外阻塞！主线程拿 state 锁永远不被这条 recv 卡住
                match rx_c.recv() {
                    Ok(msg) => {
                        // 关键：把 recv() 消费走的 msg 作为 first 传下去
                        // drain_messages 会先处理它，再 try_recv 扫剩余积压
                        pump_c(&state_c, &rx_c, Some(msg));
                    }
                    Err(RecvError) => {
                        break;
                    }
                }
            }
        });
    }

    // Step 3: 定时刷新（主线程 Slint Timer）
    //
    // 只做一件事：spawn 异步任务 → 列表 → tx.send(Msg)
    // 不直接碰 Slint Model——那样在主线程回调里改 Model 没问题，
    // 但每次刷新都会阻塞 Slint 事件循环（bollard 是 async，但 spawn 本身有开销）。
    // 统一交给 consumer 线程调 invoke_from_event_loop 更新，
    // 保持主线程回调薄、快。
    {
        let state_t = state.clone();
        Timer::default().start(TimerMode::Repeated, REFRESH_INTERVAL, move || {
            if let Ok(mut s) = state_t.lock() {
                s.refresh();
            }
            // refresh() 已经 spawn 了 tokio 异步任务，完成后 tx.send(Msg)
            // consumer 线程正在 recv() 阻塞，收到新 Msg 自动被唤醒 pump
        });
    }

    // Step 4: 初始连接 + 首屏列表
    //
    // 锁内取 tx + rt handle → 立刻 drop → spawn 两个并行列表请求
    // 结果通过 tx.send 送回，consumer 线程稍后会 pump 出来。
    {
        let Ok(s) = state.lock() else {
            return;
        };
        let tx = s.tx.clone();
        let rt = s.rt.handle().clone();
        drop(s); // 先放锁再 spawn，保证不堵 Timer/回调
        rt.spawn(async move {
            let _ = connect();
            let c_res = list_containers(true).await;
            let i_res = list_images().await;
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
        });
    }

    // Step 5: 绑定 UI 回调 → 进入 Slint 主循环
    callbacks::bind_callbacks(&ui, &state);

    // —— 进入 Slint 主循环 ——
    let _ = ui.run();
}
