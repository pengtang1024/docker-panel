//! consumer 线程 —— UI 更新泵
//!
//! mpsc::Receiver<Msg> 由本线程**独占**（不进 Mutex）：
//! recv_timeout 在锁外阻塞，主线程永远不会被卡住。
//!
//! 收到 Msg → 分批 lock state 更新数据 → drop 锁 → 节流后 invoke_from_event_loop
//! 让主线程 set Slint Model（两阶段铁律）。
//!
//! ⚠️ 日志洪峰防护（卡死修复）：
//!  1. 单次 drain 最多 MAX_DRAIN_BATCH 条，避免长时间持锁阻塞 UI 回调；
//!  2. UI 最多每 UI_MIN_INTERVAL 应用一次，把上千次 invoke 合并成每秒 ~8 次；
//!  3. 只重建本批真正变化的区域（日志洪峰不再重算容器/镜像/网络/卷/抽屉）。

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use slint::{invoke_from_event_loop, SharedString, Weak};

use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::{SharedState, REFRESH_INTERVAL};
use crate::ui_update::DrainResult;

/// UI 刷新最短间隔：日志洪峰下把上千次 invoke 合并成每秒 ~8 次
const UI_MIN_INTERVAL: Duration = Duration::from_millis(120);
/// 单次持锁最多消费的消息数（防止无界 drain 长时间占锁）
const MAX_DRAIN_BATCH: usize = 2048;
/// 锁外阻塞超时：兼顾退出检查与空闲时尽快 flush pending
const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// 下一轮 `recv_timeout` 的等待时间。
///
/// 有 pending 时只睡到节流窗口结束（最多再等 RECV_TIMEOUT，保证 shutdown 可观测）；
/// 空闲时固定 RECV_TIMEOUT。这样最坏 UI 延迟从「两个 100ms 超时」收敛到约 120ms。
fn poll_timeout(has_pending: bool, since_apply: Duration) -> Duration {
    if has_pending {
        UI_MIN_INTERVAL
            .saturating_sub(since_apply)
            .min(RECV_TIMEOUT)
    } else {
        RECV_TIMEOUT
    }
}

/// 启动 consumer 线程，返回 join handle（优雅退出时 join）
pub fn spawn(state: SharedState, rx: Receiver<Msg>, ui: Weak<MainWindow>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut pending = DrainResult::default();
        let mut last_apply = Instant::now() - UI_MIN_INTERVAL;

        loop {
            // 锁外阻塞！超时后检查 shutdown；空闲时也用于 flush pending
            let wait = poll_timeout(pending.processed > 0, last_apply.elapsed());
            let first = match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };

            // 有积压就分批消费；每批释放锁，UI 线程的点击/搜索回调不会被饿死
            let mut first = first;
            loop {
                let res = {
                    let Ok(mut s) = state.lock() else { return };
                    s.drain_messages(&rx, first.take(), MAX_DRAIN_BATCH)
                };
                pending.merge(res);
                if pending.shutdown {
                    return;
                }
                if !pending.has_more {
                    break;
                }
                if last_apply.elapsed() >= UI_MIN_INTERVAL {
                    break;
                }
            }

            // 节流应用：洪峰中多批状态更新合并成一次 UI invoke
            if pending.processed > 0 && last_apply.elapsed() >= UI_MIN_INTERVAL {
                apply_ui(&state, &ui, &mut pending);
                last_apply = Instant::now();
            }
        }
    })
}

/// 把累计状态一次性灌给 UI（锁内只 build，锁外 invoke）
fn apply_ui(state: &SharedState, ui_weak: &Weak<MainWindow>, pending: &mut DrainResult) {
    let res = std::mem::take(pending);
    let Ok(mut s) = state.lock() else { return };

    // 操作完成立即刷新（只 spawn，不做 IO）
    if res.refresh_now {
        s.refresh();
    }

    // 只重建本批变化的区域：日志洪峰时其余列表一律跳过
    let container_rows = res
        .lists_changed
        .then(|| crate::ui_rows::build_container_rows(&s));
    let image_rows = res
        .lists_changed
        .then(|| crate::ui_rows::build_image_rows(&s));
    let network_rows = res
        .lists_changed
        .then(|| crate::ui_rows::build_network_rows(&s));
    let volume_rows = res
        .lists_changed
        .then(|| crate::ui_rows::build_volume_rows(&s));
    let compose_rows = res
        .lists_changed
        .then(|| crate::ui_rows::build_compose_rows(&s));
    // 抽屉详情含 CPU/内存/磁盘（来自列表 stats），列表刷新时也要重建，
    // 但纯日志消息不触发（drawer_changed/lists_changed 都为 false）
    let drawer_data = if res.drawer_changed || res.lists_changed {
        crate::detail_fmt::build_drawer_data(&s)
    } else {
        None
    };
    let log_data = res.logs_changed.then(|| {
        let epoch = s.log_epoch.load(Ordering::Acquire);
        (
            SharedString::from(s.logs.join("\n")),
            s.log_epoch.clone(),
            epoch,
        )
    });

    let batch_count = if res.lists_changed {
        let live: Vec<String> = s.containers.iter().map(|c| c.id.clone()).collect();
        s.batch.purge_dead(live)
    } else {
        s.batch.selected_ids.len()
    };
    let running_count = s.containers.iter().filter(|c| c.is_running()).count();
    let containers_len = s.containers.len();
    let images_len = s.images.len();
    let networks_len = s.networks.len();
    let volumes_len = s.volumes.len();
    let sort_field_int = s.sort_field.to_ui_index() as i32;
    let sort_desc = s.sort_desc;
    let rt_h = s.rt.handle().clone();

    drop(s); // ⚠️ invoke_from_event_loop 之前必须释放锁

    let ui_w = ui_weak.clone();
    let ui_w_for_toast = ui_w.clone();
    invoke_from_event_loop(move || {
        let Some(ui) = ui_w.upgrade() else { return };
        ui.set_loading(false);
        // 走 render::*（thread_local 缓存模型就地同步），只 set 变化的列表
        if let Some(rows) = container_rows {
            ui.set_container_rows(crate::render::containers(rows));
        }
        if let Some(rows) = image_rows {
            ui.set_image_rows(crate::render::images(rows));
        }
        if let Some(rows) = network_rows {
            ui.set_network_rows(crate::render::networks(rows));
        }
        if let Some(rows) = volume_rows {
            ui.set_volume_rows(crate::render::volumes(rows));
        }
        if let Some(rows) = compose_rows {
            ui.set_compose_rows(crate::render::compose_rows(rows));
        }
        if let Some((text, epoch_ref, epoch)) = log_data {
            // 构建 log_text 后 UI 线程可能已经 Clear/切容器；epoch 变了就丢弃旧文本
            if epoch_ref.load(Ordering::Acquire) == epoch {
                ui.set_log_text(text);
            }
        }
        ui.set_sort_field(sort_field_int);
        ui.set_sort_desc(sort_desc);
        ui.set_batch_count(batch_count as i32);
        ui.set_total_containers(containers_len as i32);
        ui.set_running_containers(running_count as i32);
        ui.set_total_images(images_len as i32);
        ui.set_total_networks(networks_len as i32);
        ui.set_total_volumes(volumes_len as i32);
        if let Some(ok) = res.docker_ok {
            ui.set_docker_ok(ok);
        }
        if let Some(text) = res.status_text {
            ui.set_status_text(SharedString::from(text));
        }
        if res.reset_pull_busy {
            ui.set_pull_busy(false);
        }
        if res.close_pull {
            ui.set_pull_open(false);
        }
        if let Some(ref d) = drawer_data {
            crate::ui_apply::apply_drawer(&ui, d);
        }
        // Toast auto-restore：操作结果 2s 后恢复 "Ready"
        if res.toast {
            let rt_t = rt_h;
            let ui_weak_t = ui_w_for_toast;
            rt_t.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak_t.upgrade() else {
                        return;
                    };
                    ui.set_status_text(SharedString::from(crate::i18n::t("Ready")));
                })
                .ok();
            });
        }
    })
    .ok();
}

/// 3s 列表刷新 interval（容器/镜像/网络/卷统一由 s.refresh() 并发刷新）
pub fn spawn_refresh_interval(state: &SharedState) {
    let state_t = state.clone();
    let rt_h = {
        let Ok(s) = state.lock() else { return };
        s.rt.handle().clone()
    };
    rt_h.spawn(async move {
        let mut tick = tokio::time::interval(REFRESH_INTERVAL);
        tick.tick().await; // 跳过首次立即触发：首屏列表由 spawn_initial_load 负责
        loop {
            tick.tick().await;
            if let Ok(mut s) = state_t.lock() {
                s.refresh();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_uses_recv_timeout() {
        assert_eq!(poll_timeout(false, Duration::ZERO), RECV_TIMEOUT);
        assert_eq!(
            poll_timeout(false, Duration::from_millis(500)),
            RECV_TIMEOUT
        );
    }

    #[test]
    fn pending_wait_is_capped_by_recv_timeout() {
        assert_eq!(poll_timeout(true, Duration::ZERO), RECV_TIMEOUT);
    }

    #[test]
    fn pending_wait_tracks_remaining_throttle() {
        assert_eq!(
            poll_timeout(true, Duration::from_millis(100)),
            Duration::from_millis(20)
        );
    }

    #[test]
    fn due_pending_returns_zero() {
        assert_eq!(poll_timeout(true, UI_MIN_INTERVAL), Duration::ZERO);
        assert_eq!(
            poll_timeout(true, Duration::from_millis(200)),
            Duration::ZERO
        );
    }
}
