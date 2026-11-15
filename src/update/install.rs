//! 自动更新的本机侧：环境探测、下载、校验、安装、打开下载页、重启。
//!
//! 所有函数都是阻塞的（网络 / 子进程 / 文件 IO），调用方必须放在
//! `tokio::task::spawn_blocking` 里，禁止在 tokio worker 或 UI 线程直接调用。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::i18n;
use crate::update::{version, InstallEnv, InstallKind, UpdateInfo};

mod download;

pub use download::cache_dir;

/// 探测本机安装环境（只读，不改文件）。
pub fn detect_env(exe: &Path) -> InstallEnv {
    InstallEnv {
        debug_build: cfg!(debug_assertions) || path_in_target_dir(exe),
        os: std::env::consts::OS,
        arch: version::target_arch(),
        dpkg_managed: dpkg_managed(exe),
        pkexec_available: crate::update::command_exists("pkexec"),
        exe_dir_writable: exe.parent().map(dir_writable).unwrap_or(false),
    }
}

/// 下载 + 校验 + 安装。成功后清理缓存目录；失败时当前安装保持不变。
///
/// `progress(百分比, 文案)` 由调用方决定如何投递（通常是 `Msg::UpdateProgress`）。
pub fn download_and_install(
    info: &UpdateInfo,
    exe: &Path,
    cache: &Path,
    progress: &mut dyn FnMut(i32, String),
) -> Result<(), String> {
    if info.plan.kind == InstallKind::Unsupported {
        return Err(i18n::t(
            info.plan
                .reason
                .map(|r| r.msgid())
                .unwrap_or("Automatic install is not supported"),
        ));
    }
    let asset = info
        .plan
        .asset
        .as_ref()
        .ok_or_else(|| i18n::t("No installable asset for this release"))?;

    let checksums_asset = info
        .assets
        .iter()
        .find(|a| a.name == "checksums.txt")
        .ok_or_else(|| i18n::t("Release checksums.txt is missing"))?;
    let checksums = download::fetch_text(&checksums_asset.url)?;
    let expected = version::checksum_for(&checksums, &asset.name)
        .ok_or_else(|| i18n::t("Release checksums.txt does not contain this asset"))?;
    // 清单只做一致性断言：期望值仍只来自 checksums.txt，不一致即拒绝安装（fail closed）
    if let Some(m) = info.manifest.as_ref() {
        crate::update::manifest::assert_asset_sha(m, &asset.name, &expected)
            .map_err(|_| i18n::t("Update manifest does not match checksums.txt"))?;
    }

    fs::create_dir_all(cache)
        .map_err(|e| i18n::trf("Failed to create update cache: {0}", &[&e.to_string()]))?;
    let file_name = download::asset_file_name(&asset.name);
    let target = cache.join(file_name);

    progress(0, i18n::t("Downloading update..."));
    if let Err(e) = download::download_file(asset, &target, progress) {
        let _ = fs::remove_dir_all(cache);
        return Err(e);
    }

    progress(100, i18n::t("Verifying update..."));
    let actual = match download::sha256_file(&target) {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_dir_all(cache);
            return Err(e);
        }
    };
    if actual != expected {
        let _ = fs::remove_dir_all(cache);
        return Err(i18n::t("Update checksum mismatch"));
    }

    progress(100, i18n::t("Installing update..."));
    let result = match info.plan.kind {
        InstallKind::Deb => install_deb(&target),
        InstallKind::Binary => install_binary(&target, exe),
        InstallKind::Unsupported => Err(i18n::t("Automatic install is not supported")),
    };

    let _ = fs::remove_dir_all(cache);
    result
}

/// 打开 release 下载页。
pub fn open_url(url: &str) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| i18n::trf("Failed to open browser: {0}", &[&e.to_string()]))
}

/// 定位当前可执行文件路径。
///
/// Linux 下安装器（dpkg / 裸二进制自替换）都是**同路径 rename** 原子替换：运行中的旧进程
/// 读 `/proc/self/exe` 会得到 `/usr/bin/docker-panel (deleted)`（旧 inode 已从目录消失）。
/// 直接拿去 spawn 必然失败（重启按钮无反应）。剥掉后缀启动的才是同路径下替换后的新二进制。
fn current_exe_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| {
        i18n::trf(
            "Failed to locate current executable: {0}",
            &[&e.to_string()],
        )
    })?;
    if let Some(stripped) = exe.to_string_lossy().strip_suffix(" (deleted)") {
        let clean = PathBuf::from(stripped);
        if clean.is_file() {
            return Ok(clean);
        }
    }
    Ok(exe)
}

/// 更新重启的拉起延时：给旧进程留出退出时间，避免新旧窗口短暂并存。
const RELAUNCH_DELAY_SECS: u64 = 2;

/// 由独立短命进程延时拉起当前可执行文件（更新安装完成后由 `main` 调用）。
///
/// **调用前必须已释放单实例锁**（见 `single_instance::release_then`）：新进程启动时
/// 会重新 `acquire()`，锁还被旧进程持有时它只会打印 `already running` 后退出。
/// 用短命 launcher 而不是旧进程直接 spawn，是为了让拉起动作不依赖旧进程的退出速度
/// （旧进程还要 join 后台线程、drop tokio runtime）。launcher 只 sleep 一次就 `exec`，
/// 不产生重试循环 —— 锁被别的实例长期持有时新进程立刻退出，不会堆积进程。
pub fn restart_detached() -> Result<(), String> {
    let exe = current_exe_path()?;
    launcher_command(&exe, RELAUNCH_DELAY_SECS)
        .spawn()
        .map(|_| ())
        .map_err(|e| i18n::trf("Failed to restart application: {0}", &[&e.to_string()]))
}

/// 构造 launcher：`sleep N; exec <exe>`。exe 作为位置参数传入、不做字符串拼接，
/// 安装路径含空格或引号也不会被 shell 改写。
#[cfg(unix)]
fn launcher_command(exe: &Path, delay_secs: u64) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg("sleep \"$1\"; exec \"$2\"")
        .arg("sh")
        .arg(delay_secs.to_string())
        .arg(exe);
    cmd
}

#[cfg(not(unix))]
fn launcher_command(exe: &Path, _delay_secs: u64) -> Command {
    Command::new(exe)
}

fn path_in_target_dir(exe: &Path) -> bool {
    exe.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
}

fn dpkg_managed(exe: &Path) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let Ok(out) = Command::new("dpkg-query").arg("-S").arg(exe).output() else {
        return false;
    };
    out.status.success() && String::from_utf8_lossy(&out.stdout).contains("docker-panel")
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".docker-panel-probe-{}", std::process::id()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn install_deb(deb: &Path) -> Result<(), String> {
    let out = Command::new("pkexec")
        .arg("dpkg")
        .arg("-i")
        .arg(deb)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                i18n::t("pkexec not found, please install the deb manually")
            } else {
                i18n::trf("Failed to run pkexec: {0}", &[&e.to_string()])
            }
        })?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = crate::update::tail_text(&stderr, 300);
    if detail.is_empty() {
        Err(i18n::trf(
            "Package installation failed (exit code {0})",
            &[&out.status.code().unwrap_or(-1).to_string()],
        ))
    } else {
        Err(i18n::trf("Package installation failed: {0}", &[&detail]))
    }
}

#[cfg(unix)]
fn install_binary(downloaded: &Path, exe: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let dir = exe
        .parent()
        .ok_or_else(|| i18n::t("Failed to locate install directory"))?;
    let tmp = dir.join(format!(".docker-panel-update-{}", std::process::id()));
    fs::copy(downloaded, &tmp)
        .map_err(|e| i18n::trf("Failed to write new binary: {0}", &[&e.to_string()]))?;

    let mut perm = fs::metadata(&tmp)
        .map_err(|e| i18n::trf("Failed to read new binary: {0}", &[&e.to_string()]))?
        .permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&tmp, perm)
        .map_err(|e| i18n::trf("Failed to set binary permissions: {0}", &[&e.to_string()]))?;

    fs::rename(&tmp, exe).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        i18n::trf("Failed to replace binary: {0}", &[&e.to_string()])
    })
}

#[cfg(not(unix))]
fn install_binary(_downloaded: &Path, _exe: &Path) -> Result<(), String> {
    Err(i18n::t(
        "Automatic install is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests;
