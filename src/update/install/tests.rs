//! 自动更新安装流程的测试（文件 IO + file:// 下载，不联网）。

use std::fs;
use std::path::{Path, PathBuf};

use super::download;
use super::*;
use crate::models::update::{ManifestAsset, UpdateManifest};
use crate::update::{InstallKind, UpdateAsset, UpdateInfo};

#[test]
fn tail_is_char_safe() {
    assert_eq!(crate::update::tail_text("abcdef", 3), "def");
    assert_eq!(crate::update::tail_text("中文说明", 2), "说明");
    assert_eq!(crate::update::tail_text("  abc  ", 10), "abc");
}

#[cfg(unix)]
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "docker-panel-update-test-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[cfg(unix)]
/// 单测用验签关口：真实实现要联网取签名清单，这里只验证安装流程本身。
fn accept(_info: &UpdateInfo, _file: &Path, _name: &str) -> Result<(), String> {
    Ok(())
}

fn binary_info(source: &Path) -> UpdateInfo {
    let size = fs::metadata(source).unwrap().len();
    UpdateInfo {
        already_installed: false,
        version: "9.9.9".into(),
        notes: String::new(),
        page_url: String::new(),
        assets: vec![
            UpdateAsset {
                name: "docker-panel".into(),
                url: file_url(source),
                size,
            },
            UpdateAsset {
                name: "checksums.txt".into(),
                url: file_url(&source.with_file_name("checksums.txt")),
                size: 0,
            },
        ],
        plan: crate::update::InstallPlan {
            kind: InstallKind::Binary,
            asset: Some(UpdateAsset {
                name: "docker-panel".into(),
                url: file_url(source),
                size,
            }),
            reason: None,
        },
        manifest: None,
    }
}

#[test]
#[cfg(unix)]
fn launcher_execs_target_after_delay() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_dir("launcher");
    // 路径带空格：验证 exe 走位置参数而非拼进脚本字符串
    let dir = root.join("dir with space");
    fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("marker");
    let target = dir.join("target.sh");
    fs::write(
        &target,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    let mut perm = fs::metadata(&target).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&target, perm).unwrap();

    let status = launcher_command(&target, 0).status().expect("launcher");
    assert!(status.success(), "launcher 应正常 exec 目标并退出");
    assert!(marker.exists(), "目标必须在延时后被 exec");
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(unix)]
fn installs_verified_binary() {
    let dir = temp_dir("ok");
    let new_bin = dir.join("docker-panel.new");
    fs::write(&new_bin, b"new-binary").unwrap();
    let hash = download::sha256_file(&new_bin).unwrap();
    fs::write(dir.join("checksums.txt"), format!("{hash}  docker-panel\n")).unwrap();

    let exe = dir.join("docker-panel");
    fs::write(&exe, b"old-binary").unwrap();
    let info = binary_info(&new_bin);
    let cache = dir.join("cache");

    let mut steps = 0;
    let mut progress = |_p: i32, _t: String| steps += 1;
    install_with(&info, &exe, &cache, &mut progress, &accept).unwrap();

    assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
    assert!(!cache.exists(), "成功安装后应清理缓存");
    assert!(steps > 0, "至少应上报一次进度");
    let _ = fs::remove_dir_all(&dir);
}

/// 验签失败必须**在写盘之前**中止：可执行文件保持原样，缓存清掉。
#[test]
#[cfg(unix)]
fn verification_failure_leaves_exe_untouched() {
    let dir = temp_dir("unsigned");
    let new_bin = dir.join("docker-panel.new");
    fs::write(&new_bin, b"new-binary").unwrap();
    let hash = download::sha256_file(&new_bin).unwrap();
    fs::write(dir.join("checksums.txt"), format!("{hash}  docker-panel\n")).unwrap();

    let exe = dir.join("docker-panel");
    fs::write(&exe, b"old-binary").unwrap();
    let info = binary_info(&new_bin);
    let cache = dir.join("cache");

    let reject = |_i: &UpdateInfo, _f: &Path, _n: &str| Err("sig bad".to_string());
    let mut progress = |_p: i32, _t: String| {};
    let err = install_with(&info, &exe, &cache, &mut progress, &reject).unwrap_err();
    assert!(err.contains("sig bad"));
    assert_eq!(
        fs::read(&exe).unwrap(),
        b"old-binary",
        "验签失败不得改动可执行文件"
    );
    assert!(!cache.exists(), "失败后也要清理缓存");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn rejects_checksum_mismatch_without_touching_exe() {
    let dir = temp_dir("bad");
    let new_bin = dir.join("docker-panel.new");
    fs::write(&new_bin, b"new-binary").unwrap();
    fs::write(
        dir.join("checksums.txt"),
        format!("{}  docker-panel\n", "0".repeat(64)),
    )
    .unwrap();

    let exe = dir.join("docker-panel");
    fs::write(&exe, b"old-binary").unwrap();
    let info = binary_info(&new_bin);
    let cache = dir.join("cache");

    let mut progress = |_p: i32, _t: String| {};
    let err = install_with(&info, &exe, &cache, &mut progress, &accept).unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn rejects_manifest_mismatch_without_touching_exe() {
    let dir = temp_dir("manifest");
    let new_bin = dir.join("docker-panel.new");
    fs::write(&new_bin, b"new-binary").unwrap();
    // checksums.txt 与文件本身一致；清单声明的 sha256 却是别的值 → 必须拒绝
    let hash = download::sha256_file(&new_bin).unwrap();
    fs::write(dir.join("checksums.txt"), format!("{hash}  docker-panel\n")).unwrap();

    let exe = dir.join("docker-panel");
    fs::write(&exe, b"old-binary").unwrap();
    let mut info = binary_info(&new_bin);
    info.manifest = Some(UpdateManifest {
        version: "9.9.9".into(),
        notes_url: String::new(),
        deb: None,
        binary: Some(ManifestAsset {
            name: "docker-panel".into(),
            size: fs::metadata(&new_bin).unwrap().len(),
            sha256: "0".repeat(64),
        }),
    });
    let cache = dir.join("cache");

    let mut progress = |_p: i32, _t: String| {};
    let err = install_with(&info, &exe, &cache, &mut progress, &accept).unwrap_err();
    assert!(err.contains("manifest"), "{err}");
    assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
    let _ = fs::remove_dir_all(&dir);
}
