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
mod callbacks_container;
mod callbacks_image;
mod docker;
mod models;
mod slint_gen;
mod ui_apply;
mod ui_rows;
mod ui_state;
mod ui_update;

use slint_gen::*;

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use slint::{invoke_from_event_loop, ComponentHandle, ModelRc, SharedString};

use crate::docker::{
    connect, fetch_stats, list_containers, list_images, list_networks, list_volumes,
};
use crate::models::Msg;
use crate::ui_state::{AppState, SharedState, REFRESH_INTERVAL};
use crate::ui_update::derive_update;

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
        if !need_update {
            return;
        }

        // 锁内：构建 rows + 读字段
        let container_rows = ui_rows::build_container_rows(&s);
        let image_rows = ui_rows::build_image_rows(&s);
        let network_rows = ui_rows::build_network_rows(&s);
        let volume_rows = ui_rows::build_volume_rows(&s);
        let running = s.containers.iter().filter(|c| c.is_running()).count();
        let running_total = format!("{}/{}", running, s.containers.len());
        let image_total = s.images.len().to_string();
        let containers_len = s.containers.len();
        let update = derive_update(containers_len, &latest_msg);
        let detail_data = s.build_detail_data();
        let image_detail_data = s.build_image_detail_data();

        // —— 排序状态（转 int 给 Slint：Group=0 Name=1 Status=2 Cpu=3 Mem=4）——
        let (sort_field_int, sort_desc) = match s.sort_field {
            crate::models::SortField::Group => (0, s.sort_desc),
            crate::models::SortField::Name => (1, s.sort_desc),
            crate::models::SortField::Status => (2, s.sort_desc),
            crate::models::SortField::Cpu => (3, s.sort_desc),
            crate::models::SortField::Mem => (4, s.sort_desc),
        };

        // —— 日志搬运 ——
        let log_lines: Vec<LogLineRow> = s
            .logs
            .iter()
            .map(|l| LogLineRow {
                text: SharedString::from(l),
            })
            .collect();

        // —— 操作完成立即刷新 —— 不等 3s Timer
        if let Some(ref msg) = latest_msg {
            match msg {
                Msg::ActionDone(_, _) | Msg::PullDone(_) | Msg::CleanupDone(_) => {
                    s.refresh();
                }
                _ => {}
            }
        }

        drop(s); // ⚠️ 关键：必须在 invoke_from_event_loop 之前释放锁

        let ui_w = ui_w_pump.clone();
        invoke_from_event_loop(move || {
            let Some(ui) = ui_w.upgrade() else { return };
            let cm: ModelRc<ContainerRow> = ModelRc::from(container_rows.as_slice());
            let im: ModelRc<ImageRow> = ModelRc::from(image_rows.as_slice());
            let nm: ModelRc<crate::slint_gen::NetworkRow> = ModelRc::from(network_rows.as_slice());
            let vm: ModelRc<crate::slint_gen::VolumeRow> = ModelRc::from(volume_rows.as_slice());
            let lm: ModelRc<LogLineRow> = ModelRc::from(log_lines.as_slice());
            ui_apply::apply_lists(&ui, cm, im, nm, vm, lm);
            ui.set_sort_field(sort_field_int);
            ui.set_sort_desc(sort_desc);
            ui.set_container_count(SharedString::from(running_total));
            ui.set_image_count(SharedString::from(image_total));
            ui.set_docker_ok(update.docker_ok);
            ui.set_status_text(SharedString::from(update.status_text));
            if update.reset_pull_busy {
                ui.set_pull_busy(false);
            }
            if update.close_pull {
                ui.set_pull_open(false);
            }
            if let Some(ref d) = detail_data {
                ui_apply::apply_detail(&ui, d);
            }
            if let Some(ref fields) = image_detail_data {
                ui_apply::apply_image_detail(&ui, fields);
            }
        })
        .ok();
    };

    // ═══════════════════════════════════════════════════════
    // Step 2: 启动 consumer 线程（recv_timeout 200ms + shutdown flag）
    // rx_c 是 main move 进来的，consumer 独占从此 main 再也碰不到。
    // ═══════════════════════════════════════════════════════
    let consumer_handle: JoinHandle<()> = {
        let state_c = state.clone();
        let rx_c = rx; // 独占所有权，从此再也不进 Mutex
        let pump_c = pump_and_apply.clone();
        std::thread::spawn(move || {
            loop {
                // 先扫一次积压（非阻塞，无 first msg）
                pump_c(&state_c, &rx_c, None);

                // ⚠️ 锁外阻塞！200ms 超时后检查 shutdown flag
                match rx_c.recv_timeout(Duration::from_millis(200)) {
                    Ok(msg) => {
                        // 关键：把 recv() 消费走的 msg 作为 first 传下去
                        // drain_messages 会先处理它，再 try_recv 扫剩余积压
                        pump_c(&state_c, &rx_c, Some(msg));
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // 定期检查优雅退出 flag
                        let Ok(s) = state_c.lock() else { break };
                        if s.shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        })
    };

    // Step 3: 定时刷新 —— 用 tokio interval（比 Slint Timer 更可靠，软件渲染器下也能工作）
    {
        let state_t = state.clone();
        let rt_h = {
            let Ok(s) = state.lock() else {
                return;
            };
            s.rt.handle().clone()
        };
        rt_h.spawn(async move {
            let mut tick = tokio::time::interval(REFRESH_INTERVAL);
            loop {
                tick.tick().await;
                if let Ok(mut s) = state_t.lock() {
                    s.refresh();
                }
            }
        });
    }

    // Step 3b: stats 采样（6s interval）
    {
        let state_s = state.clone();
        let rt_h = {
            let Ok(s) = state_s.lock() else {
                return;
            };
            s.rt.handle().clone()
        };
        rt_h.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(6));
            tick.tick().await; // 跳过首次立即触发
            loop {
                tick.tick().await;
                let (tx, ids): (_, Vec<String>) = {
                    let Ok(s) = state_s.lock() else { continue };
                    (
                        s.tx.clone(),
                        s.containers
                            .iter()
                            .filter(|c| c.is_running())
                            .map(|c| c.id.clone())
                            .collect(),
                    )
                };
                let futs: Vec<_> = ids
                    .into_iter()
                    .map(|id| {
                        let tx = tx.clone();
                        async move {
                            if let Ok(snap) = fetch_stats(&id).await {
                                let _ = tx.send(Msg::Stats(id, snap));
                            }
                        }
                    })
                    .collect();
                futures::future::join_all(futs).await;
            }
        });
    }

    // Step 4: 初始连接 + 首屏列表 + inspect 第一个容器填充详情面板
    {
        let Ok(s) = state.lock() else {
            return;
        };
        let tx = s.tx.clone();
        let rt = s.rt.handle().clone();
        drop(s); // 先放锁再 spawn，保证不堵 Timer/回调
        rt.spawn(async move {
            let _ = connect();
            let (c_res, i_res, n_res, v_res) = futures::join!(
                list_containers(true),
                list_images(),
                list_networks(),
                list_volumes(),
            );
            let first_id = c_res
                .as_ref()
                .ok()
                .and_then(|list| list.first().map(|c| c.id.clone()));
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
            let _ = tx.send(Msg::Networks(n_res));
            let _ = tx.send(Msg::Volumes(v_res));
            // 首屏列表到齐后自动 inspect 第一个容器
            if let Some(id) = first_id {
                if let Ok(detail) = crate::docker::inspect_container(&id).await {
                    let _ = tx.send(Msg::ContainerDetail(Ok(detail)));
                }
            }
        });
    }

    // Step 5: 绑定 UI 回调 → 进入 Slint 主循环
    callbacks::bind_callbacks(&ui, &state);

    // —— 进入 Slint 主循环 ——
    let _ = ui.run();

    // ═══════════════════════════════════════════════════════
    // Step 6: 优雅退出
    //   shutdown flag → consumer 线程的 200ms 超时循环检测到 break
    //   join consumer（~250ms）→ drop state → Runtime drop（tokio 自动 shutdown）
    // ═══════════════════════════════════════════════════════
    if let Ok(s) = state.lock() {
        s.shutdown.store(true, Ordering::Relaxed);
    }
    let _ = consumer_handle.join();
}
