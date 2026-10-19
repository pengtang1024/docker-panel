//! docker-panel — 极简 Docker 桌面管理工具
//!
//! Slint 原生 GUI + 软件渲染器，零 GPU 依赖。
//!
//! 线程模型：
//! ┌──────────────┐  ui.run() 阻塞  ┌──────────────────┐
//! │  主线程       │──回调─────────▶│  Mutex<AppState>  │
//! └──────────────┘                 └────────┬─────────┘
//!                                            │ tx.send(Msg)
//! ┌──────────────┐  recv_timeout（锁外阻塞）  ┌▼──────────────────┐
//! │ consumer 线程│◀─────────────────────────│  tokio runtime     │
//! └──────────────┘                          │  3s 刷新 / 6s 统计  │
//!   详见 consumer.rs / bootstrap.rs          └───────────────────┘

mod batch;
mod bootstrap;
mod callbacks;
mod callbacks_container;
mod callbacks_drawer;
mod callbacks_image;
mod callbacks_netvol;
mod config;
mod consumer;
mod detail_fmt;
mod docker;
mod follow_runtime;
mod i18n;
mod log_status;
mod models;
mod render;
mod slint_gen;
mod theme;
mod ui_apply;
mod ui_rows;
mod ui_state;
mod ui_update;

use std::sync::atomic::Ordering;

use slint::ComponentHandle;

use crate::slint_gen::MainWindow;
use crate::ui_state::AppState;

fn main() {
    // 按系统 locale 选择语言（zh* → 简体中文，其它语言回落英文源串）
    i18n::init();

    // AppState::new() 返回 (Arc<Mutex<AppState>>, Receiver<Msg>)；
    // Receiver 交给 consumer 线程独占，绝不进 Mutex（防死锁关键）。
    let (state, rx) = AppState::new();

    let ui = MainWindow::new().expect("create window");

    // select_bundled_translation 必须在第一个组件创建之后调用
    //（GLOBAL_CONTEXT 随组件创建而初始化，之前调用只会得到 NoTranslationsBundled）；
    // 切换后 translations_dirty 会标脏，@tr 绑定的属性自动重新求值。
    if let Some(lang) = i18n::slint_language() {
        if let Err(e) = slint::select_bundled_translation(lang) {
            eprintln!("select translation {lang} failed: {e:?}");
        }
    }

    // 检测系统主题偏好，灌给 Theme.system_dark —
    // 必须在 bind_callbacks（restore_config 会设置 theme_mode）之前，
    // 否则 theme_mode=auto 时 is_dark 计算不到正确值。
    let initial_dark = theme::detect_system_dark().unwrap_or(false);
    ui.set_system_dark(initial_dark);

    // consumer 线程：Msg 泵 → UI 更新
    let consumer_handle = consumer::spawn(state.clone(), rx, ui.as_weak());
    // 后台定时任务：3s 列表刷新、6s stats 采样
    consumer::spawn_refresh_interval(&state);
    bootstrap::spawn_stats_interval(&state);
    bootstrap::spawn_volume_size_interval(&state);
    // 首屏数据
    bootstrap::spawn_initial_load(&state);

    // 系统主题热切换监听（auto 模式下每 5s poll gsettings，值翻转才推给 UI）
    let theme_monitor = bootstrap::spawn_theme_monitor(&ui, &state);

    callbacks::bind_callbacks(&ui, &state);

    let _ = ui.run();

    // 优雅退出：shutdown flag → consumer / theme-monitor 轮询发现后 break → join
    if let Ok(s) = state.lock() {
        s.shutdown.store(true, Ordering::Relaxed);
        s.persist(); // 退出落盘（搜索词等高频变更项只在此处保存）
    }
    let _ = consumer_handle.join();
    let _ = theme_monitor.join();
}
