//! docker-panel — 极简 Docker Engine 桌面管理面板
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
//! └──────────────┘                          │  events → refresh  │
//!   详见 consumer.rs / event_watch.rs         │  6s stats / 30s df │
//!                                           └───────────────────┘

mod batch;
mod bootstrap;
mod callbacks;
mod callbacks_compose;
mod callbacks_container;
mod callbacks_drawer;
mod callbacks_image;
mod callbacks_netvol;
mod callbacks_settings;
mod callbacks_sort;
mod callbacks_update;
mod cli;
mod config;
mod consumer;
mod detail_fmt;
mod docker;
mod event_watch;
mod follow_runtime;
mod format;
mod i18n;
mod lock;
mod log_status;
mod models;
mod render;
mod single_instance;
mod slint_gen;
mod theme;
mod ui_apply;
mod ui_rows;
mod ui_state;
mod ui_update;
mod update;

use std::sync::atomic::Ordering;

use slint::ComponentHandle;

use crate::slint_gen::MainWindow;
use crate::ui_state::AppState;

fn main() {
    // 命令行参数：--version / -V / --help 在此直接退出，不初始化 Slint、
    // 不连 Docker、不设置 xdg app_id；未知参数按正常启动 GUI 处理。
    match cli::parse_args(std::env::args().skip(1)) {
        cli::CliAction::Version => {
            println!("{}", cli::version_text(env!("CARGO_PKG_VERSION")));
            return;
        }
        cli::CliAction::Help => {
            print!("{}", cli::help_text());
            return;
        }
        cli::CliAction::Run => {}
    }

    // 单实例：同一用户只允许一个 GUI 实例。
    // 时机铁律：--version / --help 必须在上面的 cli 分支里就返回，
    // 不能被单实例挡住（看版本号不该要求先关掉正在跑的窗口）。
    // 拿不到锁就不再初始化 Slint / 不连 Docker，提示后以非 0 退出码结束。
    let mut guard = match single_instance::acquire() {
        Ok(single_instance::Acquire::Primary(g)) => Some(g),
        Ok(single_instance::Acquire::AlreadyRunning) => {
            eprintln!(
                "docker-panel: already running, not starting a second window \
                 (single-instance lock held by the existing process)"
            );
            std::process::exit(1);
        }
        Err(e) => {
            // 锁不可用不阻断启动：运行目录异常时宁可多开一个窗口，也不能打不开面板
            eprintln!("docker-panel: single-instance lock unavailable ({e}), continuing");
            None
        }
    };

    // 按系统 locale 选择语言（zh* → 简体中文，其它语言回落英文源串）
    i18n::init();

    // AppState::new() 返回 (Arc<Mutex<AppState>>, Receiver<Msg>)；
    // Receiver 交给 consumer 线程独占，绝不进 Mutex（防死锁关键）。
    let (state, rx) = AppState::new();

    let ui = MainWindow::new().expect("create window");

    // Wayland 下必须显式声明 xdg app_id（= desktop 文件名去掉 .desktop 后缀）：
    // 不设置时窗口没有 app_id，GNOME Shell 无法把它匹配到 docker-panel.desktop，
    // 面板坞（Dock）会显示成「未识别程序」，图标退化成通用占位图。
    // 时机：native window 在 ui.run() 里才创建，此刻设置仍然有效（早于 show）。
    // 仅 Wayland 需要：X11 下 winit 用 argv[0] 回退生成 WM_CLASS（class 段 = docker-panel），
    // 显式设置反而会让 WM_CLASS 的 class 段变空（Slint 固定传 with_name(app_id, "")）。
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Err(e) = slint::set_xdg_app_id("docker-panel") {
            eprintln!("set xdg app id failed: {e:?}");
        }
    }

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
    // 事件驱动：Docker 资源变化即时刷新（空闲零请求）
    event_watch::spawn(&state);
    // 后台定时任务：6s stats 采样、30s 卷占用（列表本身由事件驱动刷新）
    bootstrap::spawn_stats_interval(&state);
    bootstrap::spawn_disk_usage_interval(&state);
    // 首屏数据
    bootstrap::spawn_initial_load(&state);
    // 自动更新：启动 10s 后检查一次，之后每 6h 轮询（24h 节流）
    update::spawn_scheduler(&state);

    // 系统主题热切换监听（auto 模式下每 5s poll gsettings，值翻转才推给 UI）
    let theme_monitor = bootstrap::spawn_theme_monitor(&ui, &state);

    callbacks::bind_callbacks(&ui, &state);

    // 第二条实例通过锁文件旁的 socket 发来 raise 请求时，尽力把窗口带回前台。
    // Wayland 下客户端无激活权（compositor 决定是否置顶），只能做
    // show + 取消最小化 + 重绘；X11 下通常能真正提到最前。
    if let Some(g) = guard.as_mut() {
        let weak = ui.as_weak();
        g.spawn_raise_listener(move || {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    let _ = ui.show();
                    ui.window().set_minimized(false);
                    ui.window().request_redraw();
                }
            });
        });
    }

    let _ = ui.run();

    // 优雅退出：shutdown flag → consumer / theme-monitor 轮询发现后 break → join
    if let Some(s) = crate::lock::acquire(&state, "main::shutdown") {
        s.shutdown.store(true, Ordering::Relaxed);
        s.persist(); // 退出落盘（搜索词等高频变更项只在此处保存）
    }
    let _ = consumer_handle.join();
    let _ = theme_monitor.join();
}
