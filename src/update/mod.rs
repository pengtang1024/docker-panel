//! 自动更新：GitHub Releases 检查、下载、校验、安装与重启。
//!
//! 分层：
//!   - `github`：HTTP + JSON（阻塞，调用方 spawn_blocking）
//!   - `version`：纯逻辑（版本比较 / 资产选择 / checksums 解析）
//!   - `install`：本机探测 + 下载 + SHA-256 + deb/二进制安装
//!   - 本文件：任务编排 + Msg 投递 + UI 视图快照
//!
//! 线程纪律：网络、子进程、文件 IO 全部在 `spawn_blocking` 里执行；
//! 不在持锁期间做任何阻塞操作。

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::Msg;
use crate::ui_state::{AppState, SharedState};

pub mod github;
pub mod install;
pub mod markdown;
pub mod version;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 启动检查的节流周期：24 小时。
pub const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;
/// 常驻进程的轮询周期：每 6 小时看一次是否到期。
const SCHEDULER_TICK_SECS: u64 = 6 * 60 * 60;
/// 启动后延迟检查：先让首屏 Docker 数据加载完。
const STARTUP_DELAY_SECS: u64 = 10;

pub const RELEASES_PAGE: &str = "https://github.com/pengtang1024/docker-panel/releases/latest";

pub use crate::models::update::{
    InstallKind, InstallPlan, UnsupportedReason, UpdateAsset, UpdateInfo,
};

/// 本机安装环境（由 `install::detect_env` 填充；单测直接构造）。
#[derive(Debug, Clone, Copy)]
pub struct InstallEnv {
    pub debug_build: bool,
    pub os: &'static str,
    pub arch: &'static str,
    pub dpkg_managed: bool,
    pub pkexec_available: bool,
    pub exe_dir_writable: bool,
}

/// consumer 用的 UI 快照（锁内构造，锁外灌给 Slint）。
pub struct UpdateView {
    pub latest: String,
    pub notes: String,
    pub status: String,
    pub progress: i32,
    pub available: bool,
    pub installable: bool,
    pub busy: bool,
    pub restart_needed: bool,
    pub reason: String,
}

pub fn view_snapshot(s: &AppState) -> UpdateView {
    let info = s.update_info.as_ref();
    UpdateView {
        latest: info.map(|i| i.version.clone()).unwrap_or_default(),
        notes: info.map(|i| i.notes.clone()).unwrap_or_default(),
        status: s.update_status.clone(),
        progress: s.update_progress,
        available: s.update_available,
        installable: info.map(|i| i.plan.installable()).unwrap_or(false),
        busy: s.update_busy_ui,
        restart_needed: s.update_restart_needed,
        reason: info
            .and_then(|i| i.plan.reason)
            .map(|r| crate::i18n::t(r.msgid()))
            .unwrap_or_default(),
    }
}

/// 启动时检查 + 常驻轮询（24h 节流；关闭开关后不自动检查）。
pub fn spawn_scheduler(state: &SharedState) {
    let state_t = state.clone();
    let rt = {
        let Ok(s) = state_t.lock() else { return };
        s.rt.handle().clone()
    };
    rt.spawn(async move {
        tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        loop {
            let due = {
                let Ok(s) = state_t.lock() else { return };
                s.check_updates
                    && !s.update_restart_needed
                    && now_secs() - s.last_update_check >= CHECK_INTERVAL_SECS
            };
            if due {
                spawn_check(&state_t, false);
            }
            tokio::time::sleep(Duration::from_secs(SCHEDULER_TICK_SECS)).await;
        }
    });
}

/// 启动一次检查；已有任务在跑时返回 false。
pub fn spawn_check(state: &SharedState, manual: bool) -> bool {
    let (rt, tx, auto_install) = {
        let Ok(s) = state.lock() else { return false };
        if s.update_busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        (s.rt.handle().clone(), s.tx.clone(), s.auto_install)
    };
    let state_t = state.clone();
    let state_for_check = state.clone();
    rt.spawn(async move {
        let tx_task = tx.clone();
        let res = tokio::task::spawn_blocking(move || {
            let raw = match github::fetch_latest() {
                Ok(raw) => raw,
                Err(e) => {
                    let _ = tx_task.send(Msg::UpdateCheckDone(manual, false, Err(e)));
                    return;
                }
            };
            let info = match build_info(raw) {
                Ok(info) => info,
                Err(e) => {
                    let _ = tx_task.send(Msg::UpdateCheckDone(manual, false, Err(e)));
                    return;
                }
            };
            let newer = version::is_newer(&info.version, CURRENT_VERSION);
            // 读取检查完成瞬间的开关（检查期间用户可能关掉自动安装）
            let auto_now = state_for_check
                .lock()
                .map(|s| s.auto_install)
                .unwrap_or(auto_install);
            let auto = auto_now && !manual && newer && info.plan.installable();
            let _ = tx_task.send(Msg::UpdateCheckDone(manual, auto, Ok(info.clone())));
            if auto {
                install_and_report(&info, &tx_task);
            }
        })
        .await;

        if res.is_err() {
            let _ = tx.send(Msg::UpdateCheckDone(
                manual,
                false,
                Err(crate::i18n::t("Update task failed")),
            ));
        }
        if let Ok(s) = state_t.lock() {
            s.update_busy.store(false, Ordering::Release);
        }
    });
    true
}

/// 用户点击「立即更新」：用已缓存的检查结果执行下载安装。
pub fn spawn_install(state: &SharedState) -> bool {
    let info = {
        let Ok(s) = state.lock() else { return false };
        s.update_info.clone()
    };
    let Some(info) = info else {
        let tx = {
            let Ok(s) = state.lock() else { return false };
            s.tx.clone()
        };
        let _ = tx.send(Msg::UpdateDone(
            Err(crate::i18n::t("Check for updates first")),
            false,
        ));
        return false;
    };

    let (rt, tx) = {
        let Ok(s) = state.lock() else { return false };
        if s.update_busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        (s.rt.handle().clone(), s.tx.clone())
    };

    if !version::is_newer(&info.version, CURRENT_VERSION) || !info.plan.installable() {
        if let Ok(s) = state.lock() {
            s.update_busy.store(false, Ordering::Release);
        }
        let reason = info
            .plan
            .reason
            .map(|r| crate::i18n::t(r.msgid()))
            .unwrap_or_else(|| crate::i18n::t("Already up to date"));
        let _ = tx.send(Msg::UpdateDone(Err(reason), false));
        return true;
    }

    let state_t = state.clone();
    rt.spawn(async move {
        let tx_task = tx.clone();
        let res = tokio::task::spawn_blocking(move || {
            install_and_report(&info, &tx_task);
        })
        .await;
        if res.is_err() {
            let _ = tx.send(Msg::UpdateDone(
                Err(crate::i18n::t("Update task failed")),
                false,
            ));
        }
        if let Ok(s) = state_t.lock() {
            s.update_busy.store(false, Ordering::Release);
        }
    });
    true
}

/// PATH 中是否存在可执行命令（不执行它）。
pub(crate) fn command_exists(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        dir.join(name).is_file() || (cfg!(windows) && dir.join(format!("{name}.exe")).is_file())
    })
}

/// 截取文本末尾（错误信息只保留最后 max 个字符，按 UTF-8 边界）。
pub(crate) fn tail_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().skip(count - max_chars).collect()
}

/// 当前时间（unix 秒）。
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn build_info(raw: github::RawRelease) -> Result<UpdateInfo, String> {
    let version = version::normalize_tag(&raw.tag)
        .ok_or_else(|| crate::i18n::trf("Unsupported release tag: {0}", &[&raw.tag]))?;
    let exe = std::env::current_exe().unwrap_or_default();
    let env = install::detect_env(&exe);
    let plan = version::plan_install(&raw.assets, &env);
    Ok(UpdateInfo {
        version,
        notes: version::truncate_notes(&markdown::plain(&raw.notes), 8000),
        page_url: raw.page_url,
        assets: raw.assets,
        plan,
    })
}

fn install_and_report(info: &UpdateInfo, tx: &Sender<Msg>) {
    let exe = std::env::current_exe().unwrap_or_default();
    let cache = install::cache_dir(&info.version);
    let mut progress = |percent: i32, text: String| {
        let _ = tx.send(Msg::UpdateProgress(percent, text));
    };
    let result = install::download_and_install(info, &exe, &cache, &mut progress);
    let done = match result {
        Ok(()) => Ok(crate::i18n::trf(
            "Updated to {0}. Restart to apply",
            &[&info.version],
        )),
        Err(e) => Err(e),
    };
    let restart = done.is_ok();
    let _ = tx.send(Msg::UpdateDone(done, restart));
}
