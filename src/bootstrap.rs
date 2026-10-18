//! 后台任务启动：stats 采样 interval + 首屏数据加载 + 系统主题热切换监听

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::Duration;

use slint::ComponentHandle;

use crate::docker::{
    fetch_stats, inspect_container, list_containers, list_images, list_networks, list_volumes,
    volume_sizes,
};
use crate::models::Msg;
use crate::slint_gen::MainWindow;
use crate::ui_state::{SharedState, VOLUME_SIZE_INTERVAL};

/// 6s stats 采样：对所有运行中容器并发 docker.stats()，双采样 CPU% 在 consumer 侧计算
pub fn spawn_stats_interval(state: &SharedState) {
    let state_s = state.clone();
    let rt_h = {
        let Ok(s) = state_s.lock() else { return };
        s.rt.handle().clone()
    };
    rt_h.spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(6));
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

/// 30s 卷占用采样：`/system/df?type=volume` 需遍历卷目录，单独低频跑；
/// 首帧立即执行一次（否则打开卷视图时体积列会空到第一个 30s 周期）
pub fn spawn_volume_size_interval(state: &SharedState) {
    let state_s = state.clone();
    let rt_h = {
        let Ok(s) = state_s.lock() else { return };
        s.rt.handle().clone()
    };
    rt_h.spawn(async move {
        let mut tick = tokio::time::interval(VOLUME_SIZE_INTERVAL);
        loop {
            tick.tick().await;
            let tx = {
                let Ok(s) = state_s.lock() else { continue };
                s.tx.clone()
            };
            if let Ok(sizes) = volume_sizes().await {
                let _ = tx.send(Msg::VolumeSizes(sizes));
            }
        }
    });
}

/// 首屏：连接 Docker + 并发拉四类列表 + inspect 第一个容器填充详情面板
pub fn spawn_initial_load(state: &SharedState) {
    let Ok(s) = state.lock() else { return };
    let tx = s.tx.clone();
    let rt = s.rt.handle().clone();
    drop(s);
    rt.spawn(async move {
        let _ = crate::docker::connect();
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
        if let Some(id) = first_id {
            if let Ok(detail) = inspect_container(&id).await {
                let _ = tx.send(Msg::ContainerDetail(Ok(detail)));
            }
        }
    });
}

// ===== 系统主题热切换监听 =====
//
// 双轨模式：
//  1. 事件驱动（零延迟，首选）：spawn `dconf watch /org/gnome/desktop/interface/` 子进程，
//     逐行读 stdout，发现 color-scheme / gtk-theme 变更就立即 detect + UI push。
//     GNOME/Mutter/Unity 等原生走 dconf 的桌面环境命中，毫秒级响应。
//  2. 5s 轮询（安全网）：dconf 不存在 / 子进程退出 / 检测失败时自动回退到 gsettings poll。
// 两条路径共享 push_if_changed 去重。

pub fn spawn_theme_monitor(ui: &MainWindow, state: &SharedState) -> std::thread::JoinHandle<()> {
    let weak = ui.as_weak();
    let state_s = state.clone();
    std::thread::Builder::new()
        .name("theme-monitor".into())
        .spawn(move || {
            let mut last: Option<bool> = None;

            // 公共闭包：一次锁同时查 shutdown + theme_mode，降低与 consumer 的锁竞争
            let mut check_state = |g: &std::sync::MutexGuard<'_, crate::ui_state::AppState>| {
                (g.shutdown.load(Ordering::Relaxed), g.theme != 0)
            };

            // 公共闭包：检测 → 去重 → invoke
            let mut push_if_changed = |last: &mut Option<bool>, weak: &slint::Weak<MainWindow>| {
                match crate::theme::detect_system_dark() {
                    None => {} // 检测失败（gsettings 挂了）→ 保留上次值不动
                    Some(detected) => {
                        if *last == Some(detected) {
                            return;
                        }
                        *last = Some(detected);
                        let w = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = w.upgrade() {
                                ui.set_system_dark(detected);
                            }
                        });
                    }
                }
            };

            // ===== 轨 1：dconf watch 事件驱动 =====
            if let Some(mut child) = try_spawn_dconf_watch() {
                let stdout = match child.stdout.take() {
                    Some(s) => s,
                    None => {
                        let _ = child.kill();
                        return poll_loop(
                            &state_s,
                            &weak,
                            &mut last,
                            &mut check_state,
                            &mut push_if_changed,
                        );
                    }
                };
                let mut reader = BufReader::new(stdout).lines();
                loop {
                    let (shutdown, skip) = state_s
                        .lock()
                        .map(|g| check_state(&g))
                        .unwrap_or((false, true));
                    if shutdown {
                        let _ = child.kill();
                        return;
                    }
                    if skip {
                        // manual 模式：让 dconf 子进程保持运行以便切回 auto 时仍可消费，
                        // 同时每 200ms 重新检查 shutdown。
                        std::thread::sleep(Duration::from_millis(200));
                        continue;
                    }

                    // auto 模式：阻塞读下一行（dconf 事件到来时毫秒级返回）
                    let line = match reader.next() {
                        None | Some(Err(_)) => {
                            // 子进程退出了（dconf 服务重启、权限问题等）→ fallback
                            let _ = child.kill();
                            eprintln!(
                                "[theme-monitor] dconf watch exited, falling back to 5s poll"
                            );
                            return poll_loop(
                                &state_s,
                                &weak,
                                &mut last,
                                &mut check_state,
                                &mut push_if_changed,
                            );
                        }
                        Some(Ok(l)) => l,
                    };

                    if line.contains("color-scheme") || line.contains("gtk-theme") {
                        push_if_changed(&mut last, &weak);
                    }
                    // 其他键（font-name / icon-theme 等）忽略
                }
            }

            // ===== 轨 2：5s gsettings 轮询安全网 =====
            poll_loop(
                &state_s,
                &weak,
                &mut last,
                &mut check_state,
                &mut push_if_changed,
            )
        })
        .expect("spawn theme-monitor thread")
}

/// 尝试 spawn `dconf watch /org/gnome/desktop/interface/`；
/// 命令不存在（KDE/Sway 等无 GNOME 栈）或立即退出时返回 None。
fn try_spawn_dconf_watch() -> Option<Child> {
    let mut child = Command::new("dconf")
        .args(["watch", "/org/gnome/desktop/interface/"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // 给 dconf 200ms 启动时间；如果它立即退出 → 这条路走不通
    std::thread::sleep(Duration::from_millis(200));
    match child.try_wait() {
        Ok(Some(_)) => None,
        Ok(None) => Some(child),
        Err(_) => None,
    }
}

/// 5s gsettings 轮询安全网。dconf 不可用或意外退出时进入此 loop。
fn poll_loop<F1, F2>(
    state_s: &SharedState,
    weak: &slint::Weak<MainWindow>,
    last: &mut Option<bool>,
    mut check_state: F1,
    mut push_if_changed: F2,
) where
    F1: FnMut(&std::sync::MutexGuard<'_, crate::ui_state::AppState>) -> (bool, bool),
    F2: FnMut(&mut Option<bool>, &slint::Weak<MainWindow>),
{
    loop {
        std::thread::sleep(Duration::from_secs(5));

        let (shutdown, skip) = state_s
            .lock()
            .map(|g| check_state(&g))
            .unwrap_or((false, true));
        if shutdown {
            return;
        }
        if skip {
            continue;
        }
        push_if_changed(last, weak);
    }
}
