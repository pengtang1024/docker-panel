//! 种子安装形态下的更新落点：用户目录里的版本目录 + 原子切换 `current` 软链。
//!
//! 为什么需要这一层：deb 装进 `/usr/bin`（root 地盘）时应用不能自替换，但启动器
//! （`launcher/`）会在「deb 种子」和「用户副本」之间挑版本更高、**验签通过**的那份
//! exec。于是更新只要落到用户目录即可静默完成——**全程不需要 root、不需要授权框**。
//!
//! 落盘布局（与 `launcher/` 的读取契约逐字对应）：
//!
//! ```text
//! ~/.local/opt/docker-panel/
//! ├── current -> versions/<version>      # 软链，切换即生效
//! └── versions/<version>/
//!     ├── docker-panel                    # 运行体，0755
//!     ├── version                         # 版本号（一行）
//!     ├── checksums.txt                   # 与 Release 逐字节一致
//!     └── checksums.txt.sig               # 对 checksums.txt 的 Ed25519 签名（base64）
//! ```
//!
//! 写入顺序是「先建临时目录写齐四件 → 整目录 rename → 再原子替换 `current`」：
//! 任何一步失败，启动器看到的仍是上一个可用版本（最差回落到种子），不会出现
//! 「跑了一半的新版本」。

use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n;

/// 版本目录里的运行体文件名（与启动器的 `PAYLOAD_NAME` 一致）。
pub const PAYLOAD_FILE: &str = "docker-panel";
/// 版本号文件名。
pub const VERSION_FILE: &str = "version";
/// 校验和清单文件名。
pub const CHECKSUMS_FILE: &str = "checksums.txt";
/// 校验和清单签名文件名。
pub const SIGNATURE_FILE: &str = "checksums.txt.sig";
/// 指向当前版本的软链名。
pub const CURRENT_LINK: &str = "current";

/// deb 包里的种子运行体目录（启动器默认读这里）。
const SEED_DIR: &str = "/usr/lib/docker-panel/seed";
/// 系统级启动器（`/usr/bin/docker-panel` 由 deb 提供）。
const DEFAULT_LAUNCHER: &str = "/usr/bin/docker-panel";

const ENV_RUNTIME_ROOT: &str = "DOCKER_PANEL_RUNTIME_ROOT";
const ENV_LAUNCHER: &str = "DOCKER_PANEL_LAUNCHER";

/// 用户副本根目录：`$DOCKER_PANEL_RUNTIME_ROOT`，否则 `$HOME/.local/opt/docker-panel`。
pub fn runtime_root() -> PathBuf {
    if let Some(root) = std::env::var_os(ENV_RUNTIME_ROOT).filter(|s| !s.is_empty()) {
        return PathBuf::from(root);
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/opt/docker-panel"),
        None => std::env::temp_dir().join("docker-panel-runtime"),
    }
}

/// 版本目录的父目录（`<root>/versions`）。
pub fn versions_dir(root: &Path) -> PathBuf {
    root.join("versions")
}

/// 启动器路径（`$DOCKER_PANEL_LAUNCHER` 覆盖，测试用）。
pub fn launcher_path() -> PathBuf {
    std::env::var_os(ENV_LAUNCHER)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LAUNCHER))
}

/// 当前进程是否跑在「启动器管理」的布局里：deb 种子，或用户副本的版本目录。
///
/// 这两种情况下更新必须落到版本目录（而不是就地 rename 运行体），否则启动器
/// 下次仍然只会挑种子或旧副本。
pub fn managed_install(exe: &Path, root: &Path) -> bool {
    let seed = Path::new(SEED_DIR);
    if exe.starts_with(seed) {
        return true;
    }
    exe.starts_with(versions_dir(root))
}

/// 把已下载、已验签的二进制装进版本目录并切换 `current`。
///
/// `checksums` / `signature_b64` 必须来自同一个 Release（签名由调用方用内置公钥验过），
/// 这里再断言「运行体哈希 = checksums 里的 docker-panel 行」，然后原样落盘——启动器
/// 启动时还会独立复核一遍。
pub fn install(
    version: &str,
    downloaded: &Path,
    checksums: &str,
    signature_b64: &str,
    root: &Path,
) -> Result<(), String> {
    let expected = crate::update::version::checksum_for(checksums, PAYLOAD_FILE)
        .ok_or_else(|| i18n::t("Release checksums.txt does not contain this asset"))?;
    let actual = super::download::sha256_file(downloaded)?;
    if actual != expected {
        return Err(i18n::t("Update checksum mismatch"));
    }
    if signature_b64.trim().is_empty() {
        return Err(i18n::t("Release checksums.txt signature is missing"));
    }

    let versions = versions_dir(root);
    fs::create_dir_all(&versions)
        .map_err(|e| i18n::trf("Failed to create update directory: {0}", &[&e.to_string()]))?;

    // 先写临时目录，写齐四件再整目录 rename —— 半成品永远进不了 versions/
    let staging = versions.join(format!(".staging-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    write_bundle(&staging, version, downloaded, checksums, signature_b64)?;

    let target = versions.join(version);
    let _ = fs::remove_dir_all(&target);
    if let Err(e) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        return Err(i18n::trf(
            "Failed to move update into place: {0}",
            &[&e.to_string()],
        ));
    }

    // 原子替换软链：current.new -> versions/<version>，再 rename 覆盖 current
    let rel = Path::new("versions").join(version);
    let link_tmp = root.join(format!(".{CURRENT_LINK}.new"));
    let _ = fs::remove_file(&link_tmp);
    if let Err(e) = std::os::unix::fs::symlink(&rel, &link_tmp) {
        return Err(i18n::trf(
            "Failed to switch update link: {0}",
            &[&e.to_string()],
        ));
    }
    fs::rename(&link_tmp, root.join(CURRENT_LINK)).map_err(|e| {
        let _ = fs::remove_file(&link_tmp);
        i18n::trf("Failed to switch update link: {0}", &[&e.to_string()])
    })?;
    prune_older_versions(&versions, version);
    Ok(())
}

fn write_bundle(
    dir: &Path,
    version: &str,
    downloaded: &Path,
    checksums: &str,
    signature_b64: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(dir)
        .map_err(|e| i18n::trf("Failed to create update directory: {0}", &[&e.to_string()]))?;
    fs::copy(downloaded, dir.join(PAYLOAD_FILE))
        .map_err(|e| i18n::trf("Failed to write new binary: {0}", &[&e.to_string()]))?;
    let payload = dir.join(PAYLOAD_FILE);
    let mut perm = fs::metadata(&payload)
        .map_err(|e| i18n::trf("Failed to read new binary: {0}", &[&e.to_string()]))?
        .permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&payload, perm)
        .map_err(|e| i18n::trf("Failed to set binary permissions: {0}", &[&e.to_string()]))?;
    let write = |name: &str, body: &str| {
        fs::write(dir.join(name), body)
            .map_err(|e| i18n::trf("Failed to write update file: {0}", &[&e.to_string()]))
    };
    write(VERSION_FILE, &format!("{version}\n"))?;
    write(CHECKSUMS_FILE, checksums)?;
    write(SIGNATURE_FILE, &format!("{}\n", signature_b64.trim()))
}

/// 只保留最新版本：删掉 `keep` 之外的版本目录（旧版本已无入口，留着只是占磁盘）。
fn prune_older_versions(versions: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(versions) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()) == Some(keep) || !path.is_dir() {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "docker-panel-managed-test-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn payload(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "docker-panel-managed-payload-{}-{name}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\necho {name}\n")).unwrap();
        path
    }

    fn checksums_for(version: &str, binary: &Path) -> String {
        let sha = super::super::download::sha256_file(binary).unwrap();
        // 与 Release 一致的 sha256sum 格式：两个空格分隔哈希与文件名
        format!("{sha}  {PAYLOAD_FILE}\n{sha}  docker-panel_{version}_amd64.deb\n")
    }

    #[test]
    fn installs_bundle_and_switches_current() {
        let root = temp_root("switch");
        let bin = payload("switch");
        let old = versions_dir(&root).join("0.7.19");
        fs::create_dir_all(&old).unwrap();

        install("0.8.0", &bin, &checksums_for("0.8.0", &bin), "AA==", &root).unwrap();

        let dir = versions_dir(&root).join("0.8.0");
        assert!(dir.join(PAYLOAD_FILE).is_file());
        assert_eq!(
            fs::read_to_string(dir.join(VERSION_FILE)).unwrap().trim(),
            "0.8.0"
        );
        assert!(dir.join(CHECKSUMS_FILE).is_file());
        assert!(dir.join(SIGNATURE_FILE).is_file());

        let current = root.join(CURRENT_LINK);
        assert!(current.is_symlink(), "current 必须是软链");
        assert_eq!(
            fs::read_link(&current).unwrap(),
            Path::new("versions").join("0.8.0")
        );
        // 旧版本目录被清掉：只保留最新
        assert!(!old.exists(), "旧版本目录应被清理");
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let root = temp_root("mismatch");
        let bin = payload("mismatch");
        let wrong = format!("{}  {PAYLOAD_FILE}\n", "0".repeat(64));
        let err = install("0.8.0", &bin, &wrong, "AA==", &root).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!root.join(CURRENT_LINK).exists(), "失败时不能留下 current");
    }

    #[test]
    fn rejects_missing_signature_and_missing_payload_entry() {
        let root = temp_root("nosig");
        let bin = payload("nosig");
        let sums = checksums_for("0.8.0", &bin);
        assert!(install("0.8.0", &bin, &sums, "  ", &root)
            .unwrap_err()
            .contains("signature"));
        let only_deb = format!("{}  docker-panel_0.8.0_amd64.deb\n", "0".repeat(64));
        assert!(install("0.8.0", &bin, &only_deb, "AA==", &root).is_err());
    }

    #[test]
    fn detects_seed_and_version_dirs_as_managed() {
        let root = Path::new("/home/tester/.local/opt/docker-panel");
        assert!(managed_install(
            Path::new("/usr/lib/docker-panel/seed/docker-panel"),
            root
        ));
        assert!(managed_install(
            Path::new("/home/tester/.local/opt/docker-panel/versions/0.8.0/docker-panel"),
            root
        ));
        assert!(!managed_install(
            Path::new("/home/tester/.local/bin/docker-panel"),
            root
        ));
    }
}
