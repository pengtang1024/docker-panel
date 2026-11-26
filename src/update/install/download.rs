//! 更新包下载与校验：缓存目录、curl 下载、SHA-256。
//!
//! 全部是阻塞 IO，调用方必须放在 `spawn_blocking` 里。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::i18n;
use crate::update::UpdateAsset;

/// 更新缓存目录：`$XDG_CACHE_HOME/docker-panel/updates/<version>`。
///
/// **只保留最新版本**：进入这个目录前先把同级的其它版本目录删掉，装完再整目录删除，
/// 缓存里不会堆积历史安装包（与 tune 的更新目录同一口径）。
pub fn cache_dir(version: &str) -> PathBuf {
    let dir = cache_root().join(version);
    prune_older_versions(&dir);
    dir
}

fn cache_root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("docker-panel")
        .join("updates")
}

/// 删掉 `keep` 之外的所有版本目录（含旧的 `<version>-<pid>` 命名）。
fn prune_older_versions(keep: &Path) {
    let Some(parent) = keep.parent() else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !path.is_dir() {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
    }
}

pub(super) fn asset_file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("update-package")
        .to_string()
}

pub(super) fn fetch_text(url: &str) -> Result<String, String> {
    crate::update::github::curl_get(url, 20, &[])
}

pub(super) fn download_file(
    asset: &UpdateAsset,
    target: &Path,
    progress: &mut dyn FnMut(i32, String),
) -> Result<(), String> {
    let mut cmd = Command::new("curl");
    cmd.arg("-fL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg("300")
        .arg("--retry")
        .arg("2")
        .arg("--retry-delay")
        .arg("1")
        .arg("-sS")
        .arg("-o")
        .arg(target)
        .arg(&asset.url);
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| i18n::trf("Failed to run curl: {0}", &[&e.to_string()]))?;
    let mut last_pct = -1i32;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr = String::new();
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_string(&mut stderr);
                    }
                    let _ = fs::remove_file(target);
                    return Err(crate::update::github::curl_error(
                        "Update download",
                        status.code(),
                        &stderr,
                    ));
                }
                break;
            }
            Ok(None) => {
                let done = fs::metadata(target).map(|m| m.len()).unwrap_or(0);
                let pct = done
                    .saturating_mul(100)
                    .checked_div(asset.size)
                    .map(|p| p.min(100) as i32)
                    .unwrap_or(0);
                if pct != last_pct {
                    last_pct = pct;
                    progress(
                        pct,
                        i18n::trf("Downloading update... {0}%", &[&pct.to_string()]),
                    );
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = fs::remove_file(target);
                return Err(i18n::trf(
                    "Failed to download update: {0}",
                    &[&e.to_string()],
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| i18n::trf("Failed to open update file: {0}", &[&e.to_string()]))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| i18n::trf("Failed to read update file: {0}", &[&e.to_string()]))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_is_sanitized() {
        assert_eq!(asset_file_name("docker-panel"), "docker-panel");
        assert_eq!(asset_file_name("../../etc/passwd"), "passwd");
        assert_eq!(asset_file_name(""), "update-package");
    }
}
