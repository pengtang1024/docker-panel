//! 更新发现的三级回退链（docs/arch/update-manifest.md §5）。
//!
//! 1. `releases/latest/download/version.json` —— 发布即生效，零 API 配额；
//! 2. `cdn.jsdelivr.net/...@master/version.json` —— 国内可达性好，最长 12h 陈旧；
//! 3. `api.github.com/.../releases/latest` —— 现役路径，信息最全。
//!
//! 上一级「可解析且结构合法」即停；只有请求失败 / JSON 非法 / 结构不认识才进下一级。
//! 阻塞调用，调用方必须放在 `spawn_blocking` 里。

use crate::i18n;
use crate::models::update::UpdateManifest;
use crate::signature;
use crate::update::{github, manifest, version, CURRENT_VERSION};

pub const MANIFEST_LATEST_URL: &str =
    "https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json";
pub const MANIFEST_CDN_URL: &str =
    "https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json";
/// 上面两个清单的 detached Ed25519 签名（发布流水线生成，随 Release 发布）。
pub const MANIFEST_SIG_LATEST_URL: &str =
    "https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json.sig";
pub const MANIFEST_SIG_CDN_URL: &str =
    "https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json.sig";
/// detached signature 的固定长度（Ed25519 = 64 字节，留一点余量给空白）。
const SIGNATURE_MAX_BYTES: usize = 1024;

const FETCH_TIMEOUT_SECS: u64 = 20;

/// 按三级回退链取最新 release 信息。
pub fn fetch_release() -> Result<github::RawRelease, String> {
    fetch_release_with(&|url| github::curl_get(url, FETCH_TIMEOUT_SECS, &[]))
}

/// 可注入 fetcher 的编排（单测覆盖回退顺序，不联网）。
fn fetch_release_with(
    fetch: &dyn Fn(&str) -> Result<String, String>,
) -> Result<github::RawRelease, String> {
    let mut raw = discover(fetch)?;
    // 清单不承载更新说明：只有「确实有新版本」时才多拉一次 API 补 notes / 下载页，
    // 失败不影响发现结果（安装所需的 URL 与校验和都已由 version 派生）。
    if raw.manifest.is_some() && version::is_newer(&raw.tag, CURRENT_VERSION) {
        if let Ok(text) = fetch(github::API_LATEST) {
            enrich_from_api(&mut raw, &text);
        }
    }
    Ok(raw)
}

/// 清单两级 → API 兜底。全失败时返回最后一级（API）的错误，沿用现有报错语义。
fn discover(fetch: &dyn Fn(&str) -> Result<String, String>) -> Result<github::RawRelease, String> {
    for url in [MANIFEST_LATEST_URL, MANIFEST_CDN_URL] {
        match fetch(url).and_then(|text| manifest::parse_manifest(&text)) {
            Ok(m) => return Ok(raw_from_manifest(m)),
            Err(e) => eprintln!("[update] manifest {url} unavailable: {e}"),
        }
    }
    let text = fetch(github::API_LATEST)?;
    github::parse_release(&text)
}

fn raw_from_manifest(m: UpdateManifest) -> github::RawRelease {
    github::RawRelease {
        tag: m.version.clone(),
        notes: String::new(),
        page_url: m.notes_url.clone(),
        assets: manifest::assets_from(&m),
        manifest: Some(m),
    }
}

/// 用 API 的更新说明补全清单结果；版本不一致（清单陈旧）时一概不采用。
fn enrich_from_api(raw: &mut github::RawRelease, api_text: &str) {
    let Ok(api) = github::parse_release(api_text) else {
        return;
    };
    let declared = raw
        .manifest
        .as_ref()
        .map(|m| m.version.as_str())
        .unwrap_or_default();
    if version::normalize_tag(&api.tag).as_deref() != Some(declared) {
        return;
    }
    if raw.notes.trim().is_empty() && !api.notes.trim().is_empty() {
        raw.notes = api.notes;
    }
    if !api.page_url.trim().is_empty() {
        raw.page_url = api.page_url;
    }
}

/// 取「已签名」的更新清单原文与 detached signature。
///
/// fail closed：两个源都拿不到**配对**的清单+签名、或验签不过，就返回错误，
/// 调用方（`install`）必须放弃安装。清单原文必须逐字节保留——重新序列化会让
/// 签名失效，所以这里返回 `String` 而不是解析后的结构体。
pub fn fetch_signed_manifest() -> Result<(String, Vec<u8>), String> {
    fetch_signed_manifest_with(&|url, max| {
        github::curl_get_bytes(url, FETCH_TIMEOUT_SECS, &[], max)
    })
}

/// 拉取「URL + 字节上限」的函数：生产环境是 `github::curl_get_bytes`，
/// 单测注入桩函数。
type BytesFetcher<'a> = &'a dyn Fn(&str, usize) -> Result<Vec<u8>, String>;

fn fetch_signed_manifest_with(fetch: BytesFetcher<'_>) -> Result<(String, Vec<u8>), String> {
    let mut last = String::new();
    for (manifest_url, sig_url) in [
        (MANIFEST_LATEST_URL, MANIFEST_SIG_LATEST_URL),
        (MANIFEST_CDN_URL, MANIFEST_SIG_CDN_URL),
    ] {
        let fetched = (
            fetch(manifest_url, signature::MANIFEST_MAX_BYTES),
            fetch(sig_url, SIGNATURE_MAX_BYTES),
        );
        match fetched {
            (Ok(manifest), Ok(sig)) => {
                match signature::verify_manifest_signature(&manifest, &sig) {
                    Ok(()) => match String::from_utf8(manifest) {
                        Ok(text) => return Ok((text, sig)),
                        Err(_) => last = "manifest is not UTF-8".to_string(),
                    },
                    Err(e) => {
                        eprintln!("[update] manifest signature from {manifest_url} rejected: {e}");
                        last = e;
                    }
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("[update] signed manifest source unavailable: {e}");
                last = e;
            }
        }
    }
    Err(i18n::trf(
        "Update manifest signature is not verifiable: {0}",
        &[&last],
    ))
}

#[cfg(test)]
mod tests;
