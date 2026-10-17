//! consumer 线程 —— UI 更新泵
//!
//! mpsc::Receiver<Msg> 由本线程**独占**（不进 Mutex）：
//! recv_timeout(200ms) 在锁外阻塞，主线程永远不会被卡住。
//!
//! 收到 Msg → lock state → drain_messages + build rows → drop 锁 →
//! invoke_from_event_loop 让主线程 set Slint Model（两阶段铁律）。

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use slint::{invoke_from_event_loop, SharedString, Weak};

use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::{SharedState, REFRESH_INTERVAL};
use crate::ui_update::derive_update;

/// 启动 consumer 线程，返回 join handle（优雅退出时 join）
pub fn spawn(state: SharedState, rx: Receiver<Msg>, ui: Weak<MainWindow>) -> JoinHandle<()> {
    std::thread::spawn(move || loop {
        // 先扫一次积压（非阻塞，无 first msg）
        pump_and_apply(&state, &rx, &ui, None);

        // ⚠️ 锁外阻塞！200ms 超时后检查 shutdown flag
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(msg) => pump_and_apply(&state, &rx, &ui, Some(msg)),
            Err(RecvTimeoutError::Timeout) => {
                let Ok(s) = state.lock() else { break };
                if s.shutdown.load(Ordering::Relaxed) {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    })
}

/// 收一批 Msg → 更新 AppState → 构建 rows → 派发主线程 UI 更新
fn pump_and_apply(
    state: &SharedState,
    rx: &Receiver<Msg>,
    ui_weak: &Weak<MainWindow>,
    first: Option<Msg>,
) {
    let Ok(mut s) = state.lock() else { return };
    let (need_update, latest_msg) = s.drain_messages(rx, first);
    if !need_update {
        return;
    }

    let batch_count = {
        let live: Vec<String> = s.containers.iter().map(|c| c.id.clone()).collect();
        s.batch.purge_dead(live)
    };
    let container_rows = crate::ui_rows::build_container_rows(&s);
    let image_rows = crate::ui_rows::build_image_rows(&s);
    let network_rows = crate::ui_rows::build_network_rows(&s);
    let volume_rows = crate::ui_rows::build_volume_rows(&s);
    let compose_rows = crate::ui_rows::build_compose_rows(&s);
    let running_count = s.containers.iter().filter(|c| c.is_running()).count();
    let containers_len = s.containers.len();
    let images_len = s.images.len();
    let networks_len = s.networks.len();
    let volumes_len = s.volumes.len();
    let update = derive_update(containers_len, &latest_msg);
    // 抽屉内容按当前选中类型生成（容器/镜像/网络/卷四选一）
    let drawer_data = crate::detail_fmt::build_drawer_data(&s);
    let sort_field_int = s.sort_field.to_ui_index() as i32;
    let sort_desc = s.sort_desc;

    let log_lines: Vec<LogLineRow> = s
        .logs
        .iter()
        .map(|l| LogLineRow {
            text: SharedString::from(l),
        })
        .collect();

    // —— 操作完成立即刷新 —— 不等 3s Timer
    if matches!(
        latest_msg,
        Some(Msg::ActionDone(_, _) | Msg::PullDone(_) | Msg::CleanupDone(_))
    ) {
        s.refresh();
    }

    drop(s); // ⚠️ 关键：invoke_from_event_loop 之前必须释放锁

    let ui_w = ui_weak.clone();
    invoke_from_event_loop(move || {
        let Some(ui) = ui_w.upgrade() else { return };
        // 走 render::*（thread_local 缓存模型就地同步）而非 ModelRc::from：
        // 后者每次都是新模型对象，Slint 会重置 Repeater 销毁全部行组件
        let cm = crate::render::containers(container_rows);
        let im = crate::render::images(image_rows);
        let nm = crate::render::networks(network_rows);
        let vm = crate::render::volumes(volume_rows);
        let lm = crate::render::logs(log_lines);
        crate::ui_apply::apply_lists(&ui, cm, im, nm, vm, lm);
        ui.set_compose_rows(crate::render::compose_rows(compose_rows));
        ui.set_sort_field(sort_field_int);
        ui.set_sort_desc(sort_desc);
        ui.set_batch_count(batch_count as i32);
        ui.set_total_containers(containers_len as i32);
        ui.set_running_containers(running_count as i32);
        ui.set_total_images(images_len as i32);
        ui.set_total_networks(networks_len as i32);
        ui.set_total_volumes(volumes_len as i32);
        ui.set_docker_ok(update.docker_ok);
        ui.set_status_text(SharedString::from(update.status_text.as_str()));
        if update.reset_pull_busy {
            ui.set_pull_busy(false);
        }
        if update.close_pull {
            ui.set_pull_open(false);
        }
        if let Some(ref d) = drawer_data {
            crate::ui_apply::apply_drawer(&ui, d);
        }
    })
    .ok();
}

/// 3s 列表刷新 interval（只刷容器+镜像，网络/卷由手动/操作后刷新）
pub fn spawn_refresh_interval(state: &SharedState) {
    let state_t = state.clone();
    let rt_h = {
        let Ok(s) = state.lock() else { return };
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
