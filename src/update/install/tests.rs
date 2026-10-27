//! 自动更新安装流程的测试（文件 IO + file:// 下载，不联网）。

use std::fs;
use std::path::{Path, PathBuf};

use super::download;
use super::*;
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
fn binary_info(source: &Path) -> UpdateInfo {
    let size = fs::metadata(source).unwrap().len();
    UpdateInfo {
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
    }
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
    download_and_install(&info, &exe, &cache, &mut progress).unwrap();

    assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
    assert!(!cache.exists(), "成功安装后应清理缓存");
    assert!(steps > 0, "至少应上报一次进度");
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
    let err = download_and_install(&info, &exe, &cache, &mut progress).unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
    let _ = fs::remove_dir_all(&dir);
}
