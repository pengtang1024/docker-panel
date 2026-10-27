//! GitHub Releases API 客户端（HTTP 走系统 curl，零 TLS 依赖）。
//!
//! 只做一件事：把 `releases/latest` 解析成 `RawRelease`。
//! 阻塞调用，调用方必须放在 `spawn_blocking` 里。

use std::process::Command;

use serde_json::Value;

use crate::i18n;
use crate::update::{command_exists, tail_text, UpdateAsset};

const API_LATEST: &str = "https://api.github.com/repos/pengtang1024/docker-panel/releases/latest";
const USER_AGENT: &str = concat!("docker-panel/", env!("CARGO_PKG_VERSION"));

/// GitHub release 的原始字段（未经本机环境判定）。
#[derive(Debug, Clone)]
pub struct RawRelease {
    pub tag: String,
    pub notes: String,
    pub page_url: String,
    pub assets: Vec<UpdateAsset>,
}

/// 拉取最新正式 release（GitHub 的 latest 不返回 draft / prerelease）。
pub fn fetch_latest() -> Result<RawRelease, String> {
    let text = curl_get(
        API_LATEST,
        20,
        &[
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
        ],
    )?;
    parse_release(&text)
}

/// 用系统 curl 拉取文本（GitHub API / checksums.txt 等小文件）。
pub(crate) fn curl_get(
    url: &str,
    timeout_secs: u64,
    headers: &[(&str, &str)],
) -> Result<String, String> {
    if !command_exists("curl") {
        return Err(i18n::t(
            "curl is required to check for updates, please install curl",
        ));
    }
    let mut cmd = Command::new("curl");
    cmd.arg("-fsSL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .arg("--max-filesize")
        .arg("2097152")
        .arg("-A")
        .arg(USER_AGENT);
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);

    let out = cmd
        .output()
        .map_err(|e| i18n::trf("Failed to run curl: {0}", &[&e.to_string()]))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(curl_error("Update check", out.status.code(), &stderr));
    }
    String::from_utf8(out.stdout)
        .map_err(|e| i18n::trf("Malformed update response: {0}", &[&e.to_string()]))
}

/// curl 失败 → 用户可读的文案。`action` 是 msgid（Update check / Update download）。
pub(crate) fn curl_error(action: &str, code: Option<i32>, stderr: &str) -> String {
    let action = i18n::t(action);
    match code {
        Some(6) => i18n::trf("{0} failed: cannot resolve host", &[&action]),
        Some(7) => i18n::trf("{0} failed: cannot connect", &[&action]),
        Some(22) => i18n::trf("{0} failed: server returned an HTTP error", &[&action]),
        Some(28) => i18n::t("Update request timed out"),
        Some(63) => i18n::t("Update response is too large"),
        Some(code) => {
            let detail = tail_text(stderr, 300);
            if detail.is_empty() {
                i18n::trf(
                    "{0} failed (curl exit code {1})",
                    &[&action, &code.to_string()],
                )
            } else {
                i18n::trf("{0} failed: {1}", &[&action, &detail])
            }
        }
        None => i18n::trf("{0} failed", &[&action]),
    }
}

/// 解析 release JSON（独立成函数便于单测，不联网）。
pub fn parse_release(text: &str) -> Result<RawRelease, String> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| i18n::trf("Malformed update response: {0}", &[&e.to_string()]))?;

    let tag = v
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| i18n::t("Malformed update response: missing tag_name"))?
        .to_string();

    let notes = v
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let page_url = v
        .get("html_url")
        .and_then(Value::as_str)
        .unwrap_or("https://github.com/pengtang1024/docker-panel/releases/latest")
        .to_string();

    let assets = v
        .get("assets")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let name = a.get("name").and_then(Value::as_str)?;
                    let url = a.get("browser_download_url").and_then(Value::as_str)?;
                    let size = a.get("size").and_then(Value::as_u64).unwrap_or(0);
                    Some(UpdateAsset {
                        name: name.to_string(),
                        url: url.to_string(),
                        size,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(RawRelease {
        tag,
        notes,
        page_url,
        assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_payload() {
        let text = serde_json::json!({
            "tag_name": "v0.6.0",
            "body": "### Added\n- auto update",
            "html_url": "https://github.com/pengtang1024/docker-panel/releases/tag/v0.6.0",
            "assets": [
                {"name": "docker-panel_0.6.0_amd64.deb", "browser_download_url": "https://example.invalid/a.deb", "size": 123},
                {"name": "checksums.txt", "browser_download_url": "https://example.invalid/checksums.txt"}
            ]
        })
        .to_string();
        let r = parse_release(&text).unwrap();
        assert_eq!(r.tag, "v0.6.0");
        assert_eq!(r.assets.len(), 2);
        assert_eq!(r.assets[0].size, 123);
        assert_eq!(r.assets[1].size, 0);
        assert!(r.notes.contains("auto update"));
    }

    #[test]
    fn rejects_malformed_payload() {
        assert!(parse_release("not json").is_err());
        assert!(parse_release("{}").is_err());
        assert!(parse_release(r#"{"tag_name": ""}"#).is_err());
    }

    #[test]
    fn maps_curl_exit_codes() {
        assert!(curl_error("Update check", Some(6), "").contains("resolve"));
        assert!(curl_error("Update download", Some(22), "").contains("HTTP"));
        assert!(curl_error("Update check", Some(28), "").contains("timed out"));
    }
}
