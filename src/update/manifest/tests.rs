//! `parse_manifest` / 资产派生的纯函数单测（不联网、不读文件）。

use super::*;

fn sha(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

fn manifest_json(version: &str, code: i64) -> serde_json::Value {
    serde_json::json!({
        "schema": 1,
        "version": version,
        "version_code": code,
        "channel": "stable",
        "released_at": "2026-11-15T02:00:00Z",
        "mandatory": false,
        "notes_url": "https://github.com/pengtang1024/docker-panel/releases/tag/0.7.13",
        "assets": {
            "deb": {
                "name": "docker-panel_0.7.13_amd64.deb",
                "size": 4096,
                "sha256": sha('a'),
            },
            "binary": { "name": "docker-panel", "size": 2048, "sha256": sha('b') }
        }
    })
}

#[test]
fn parses_valid_manifest() {
    let m = parse_manifest(&manifest_json("0.7.13", 7_013).to_string()).unwrap();
    assert_eq!(m.version, "0.7.13");
    assert_eq!(m.deb.as_ref().unwrap().size, 4096);
    assert_eq!(m.binary.as_ref().unwrap().name, "docker-panel");

    let assets = assets_from(&m);
    assert_eq!(assets.len(), 3);
    assert_eq!(
        assets[0].url,
        "https://github.com/pengtang1024/docker-panel/releases/download/0.7.13/docker-panel_0.7.13_amd64.deb"
    );
    assert_eq!(assets[0].size, 4096);
    // checksums.txt 由 version 派生：plan_install / install 都要求它存在
    assert_eq!(assets[2].name, "checksums.txt");
    assert!(assets[2].url.ends_with("/0.7.13/checksums.txt"));
}

#[test]
fn accepts_legacy_tag_with_or_without_v_prefix() {
    let plain = parse_manifest(&manifest_json("0.7.13", 7_013).to_string()).unwrap();
    let tagged = parse_manifest(&manifest_json("v0.7.13", 7_013).to_string()).unwrap();
    assert_eq!(plain.version, "0.7.13");
    assert_eq!(tagged.version, "0.7.13");
    // 派生 URL 一律用归一化后的版本（本仓库 tag 无 v 前缀）
    assert_eq!(assets_from(&plain)[0].url, assets_from(&tagged)[0].url);
}

#[test]
fn rejects_missing_fields() {
    let mut v = manifest_json("0.7.13", 7_013);
    v.as_object_mut().unwrap().remove("schema");
    assert!(parse_manifest(&v.to_string()).is_err());

    let mut v = manifest_json("0.7.13", 7_013);
    v["assets"]["deb"].as_object_mut().unwrap().remove("sha256");
    assert!(parse_manifest(&v.to_string()).is_err());

    let mut v = manifest_json("0.7.13", 7_013);
    v["assets"] = serde_json::json!({});
    assert!(parse_manifest(&v.to_string()).is_err());

    assert!(parse_manifest("not json").is_err());
}

#[test]
fn rejects_unknown_schema_or_channel() {
    let mut v = manifest_json("0.7.13", 7_013);
    v["schema"] = serde_json::json!(2);
    assert!(parse_manifest(&v.to_string()).is_err());

    let mut v = manifest_json("0.7.13", 7_013);
    v["channel"] = serde_json::json!("beta");
    assert!(parse_manifest(&v.to_string()).is_err());

    // version_code 与 version 不一致：宁可整份丢弃（回落下一级）
    let v = manifest_json("0.7.13", 7_014);
    assert!(parse_manifest(&v.to_string()).is_err());
}

#[test]
fn rejects_asset_name_with_path() {
    let mut v = manifest_json("0.7.13", 7_013);
    v["assets"]["binary"]["name"] = serde_json::json!("../docker-panel");
    assert!(parse_manifest(&v.to_string()).is_err());
}

#[test]
fn rejects_sha256_mismatch_against_checksums() {
    let m = parse_manifest(&manifest_json("0.7.13", 7_013).to_string()).unwrap();
    assert!(assert_asset_sha(&m, "docker-panel", &sha('b')).is_ok());
    // 大小写不敏感
    assert!(assert_asset_sha(&m, "docker-panel", &sha('B')).is_ok());
    assert!(assert_asset_sha(&m, "docker-panel", &sha('c')).is_err());
    // 清单没声明的资产同样拒绝（能走到下载说明本该声明）
    assert!(assert_asset_sha(&m, "checksums.txt", &sha('a')).is_err());
}

#[test]
fn lower_version_is_not_offered() {
    let m = parse_manifest(&manifest_json("0.7.11", 7_011).to_string()).unwrap();
    // 版本回退：清单版本低于当前版本时不得提示更新
    assert!(!version::is_newer(&m.version, "0.7.12"));
    assert!(version::is_newer(&m.version, "0.7.10"));
}

#[test]
fn notes_url_is_restricted_to_this_repo() {
    let mut v = manifest_json("0.7.13", 7_013);
    v["notes_url"] = serde_json::json!("https://evil.invalid/x");
    let m = parse_manifest(&v.to_string()).unwrap();
    assert_eq!(
        m.notes_url,
        "https://github.com/pengtang1024/docker-panel/releases/tag/0.7.13"
    );

    // 缺省也回落到派生值
    v.as_object_mut().unwrap().remove("notes_url");
    let m = parse_manifest(&v.to_string()).unwrap();
    assert!(m.notes_url.ends_with("/0.7.13"));
}
