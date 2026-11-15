//! 三级回退链的编排单测（注入假 fetcher，不联网）。

use std::cell::RefCell;

use super::*;

fn sha(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

/// 合法清单（可用 `replace` 改坏某个字段）。
fn manifest_text(version: &str, code: i64) -> String {
    serde_json::json!({
        "schema": 1,
        "version": version,
        "version_code": code,
        "channel": "stable",
        "released_at": "2026-11-15T02:00:00Z",
        "mandatory": false,
        "notes_url": format!("https://github.com/pengtang1024/docker-panel/releases/tag/{version}"),
        "assets": {
            "deb": {"name": format!("docker-panel_{version}_amd64.deb"), "size": 1, "sha256": sha('a')},
            "binary": {"name": "docker-panel", "size": 2, "sha256": sha('b')}
        }
    })
    .to_string()
}

fn api_text(tag: &str) -> String {
    serde_json::json!({
        "tag_name": tag,
        "body": "### Added\n- something new",
        "html_url": format!("https://github.com/pengtang1024/docker-panel/releases/tag/{tag}"),
        "assets": [
            {"name": "docker-panel_9.9.9_amd64.deb", "browser_download_url": "https://example.invalid/a.deb", "size": 1},
            {"name": "checksums.txt", "browser_download_url": "https://example.invalid/checksums.txt"}
        ]
    })
    .to_string()
}

/// 假 fetcher：按 URL 返回预置结果，并记录调用顺序。
struct Fake {
    routes: Vec<(&'static str, Result<String, String>)>,
    calls: RefCell<Vec<String>>,
}

impl Fake {
    fn new(routes: Vec<(&'static str, Result<String, String>)>) -> Self {
        Self {
            routes,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn fetch(&self, url: &str) -> Result<String, String> {
        self.calls.borrow_mut().push(url.to_string());
        self.routes
            .iter()
            .find(|(u, _)| *u == url)
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| Err("404".into()))
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

#[test]
fn first_manifest_level_wins_without_extra_requests() {
    // 用「不新于当前版本」的清单：只验证回退顺序，不触发补更新说明的 API 请求
    let fake = Fake::new(vec![(MANIFEST_LATEST_URL, Ok(manifest_text("0.0.1", 1)))]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();

    assert_eq!(raw.tag, "0.0.1");
    assert!(raw.manifest.is_some());
    // 资产 URL 由版本派生 + checksums.txt 自动补齐
    assert_eq!(raw.assets.len(), 3);
    assert!(raw.assets[2].url.ends_with("/0.0.1/checksums.txt"));
    assert_eq!(fake.calls(), vec![MANIFEST_LATEST_URL.to_string()]);
}

#[test]
fn falls_to_cdn_then_api() {
    // 1 级 404 → 2 级命中
    let fake = Fake::new(vec![(MANIFEST_CDN_URL, Ok(manifest_text("0.0.1", 1)))]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert!(raw.manifest.is_some());
    assert_eq!(
        fake.calls(),
        vec![
            MANIFEST_LATEST_URL.to_string(),
            MANIFEST_CDN_URL.to_string()
        ]
    );

    // 1、2 级都失败 → 3 级 API（manifest 为 None）
    let fake = Fake::new(vec![(github::API_LATEST, Ok(api_text("v9.9.9")))]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert!(raw.manifest.is_none());
    assert_eq!(raw.tag, "v9.9.9");
    assert_eq!(
        fake.calls(),
        vec![
            MANIFEST_LATEST_URL.to_string(),
            MANIFEST_CDN_URL.to_string(),
            github::API_LATEST.to_string()
        ]
    );
}

#[test]
fn all_levels_failing_returns_api_error() {
    let fake = Fake::new(vec![(github::API_LATEST, Err("curl: HTTP error".into()))]);
    let err = fetch_release_with(&|u| fake.fetch(u)).unwrap_err();
    assert!(err.contains("HTTP"), "{err}");
}

#[test]
fn invalid_manifest_structure_falls_through() {
    // schema 不认识 → 整份丢弃，进下一级（这里 2 级也没有 → API 兜底）
    let bad = manifest_text("9.9.9", 9_009_009).replace("\"schema\":1", "\"schema\":42");
    let fake = Fake::new(vec![
        (MANIFEST_LATEST_URL, Ok(bad)),
        (github::API_LATEST, Ok(api_text("v9.9.9"))),
    ]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert!(raw.manifest.is_none());
    assert_eq!(raw.assets.len(), 2);
}

#[test]
fn newer_manifest_is_enriched_by_api_best_effort() {
    let fake = Fake::new(vec![
        (MANIFEST_LATEST_URL, Ok(manifest_text("9.9.9", 9_009_009))),
        (github::API_LATEST, Ok(api_text("9.9.9"))),
    ]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert!(raw.notes.contains("something new"));
    assert!(raw.page_url.ends_with("/tag/9.9.9"));
    // 资产仍用清单派生（URL 不被 API 覆盖），并保留清单做一致性断言
    assert!(raw.assets[0]
        .url
        .starts_with("https://github.com/pengtang1024/docker-panel/releases/download/9.9.9/"));
    assert!(raw.manifest.is_some());
}

#[test]
fn api_with_different_version_does_not_overwrite_notes() {
    let fake = Fake::new(vec![
        (MANIFEST_LATEST_URL, Ok(manifest_text("9.9.9", 9_009_009))),
        (github::API_LATEST, Ok(api_text("v8.8.8"))),
    ]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert!(raw.notes.is_empty());
    assert!(raw.page_url.ends_with("/tag/9.9.9"));
}

#[test]
fn non_newer_manifest_does_not_hit_api() {
    let fake = Fake::new(vec![(MANIFEST_LATEST_URL, Ok(manifest_text("0.0.1", 1)))]);
    let raw = fetch_release_with(&|u| fake.fetch(u)).unwrap();
    assert_eq!(raw.tag, "0.0.1");
    assert!(raw.notes.is_empty());
    // 已是最新：只发一次请求（不回落 API）
    assert_eq!(fake.calls().len(), 1);
}
