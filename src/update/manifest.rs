//! 静态更新清单（`version.json`）的解析与资产派生。
//!
//! 清单是**不可信**的版本号广播（docs/arch/update-manifest.md）：
//!   - 资产 URL 由 `version` + 资产名派生，清单里刻意没有 `download_url`；
//!   - `sha256` / `size` 只用于与 Release 的 `checksums.txt` 交叉核对，
//!     `checksums.txt` 仍是唯一的校验源（`install.rs` 的校验路径不经过这里）。
//!
//! 本模块全是纯函数，不做任何 IO。

use serde_json::Value;

use crate::models::update::{ManifestAsset, UpdateAsset, UpdateManifest};
use crate::update::version;

/// 认识的清单结构版本；不认识就整份丢弃（回落下一级）。
pub const SCHEMA: i64 = 1;
/// 本期只接受稳定通道。
pub const CHANNEL_STABLE: &str = "stable";

const ASSET_BASE: &str = "https://github.com/pengtang1024/docker-panel/releases/download";
const TAG_BASE: &str = "https://github.com/pengtang1024/docker-panel/releases/tag";
/// `notes_url` 只允许本仓库域名前缀，其它一律改回派生值。
const NOTES_PREFIX: &str = "https://github.com/pengtang1024/docker-panel/";
const CHECKSUMS_NAME: &str = "checksums.txt";

/// `major * 10^6 + minor * 10^3 + patch`：清单里与 `version` 交叉校验的备用数字键。
pub fn version_code(version: &str) -> Option<i64> {
    let (major, minor, patch) = version::parse_version(version)?;
    Some((major * 1_000_000 + minor * 1_000 + patch) as i64)
}

/// 解析清单文本。失败信息只用于 `eprintln` 诊断（不进用户界面）。
pub fn parse_manifest(text: &str) -> Result<UpdateManifest, String> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| format!("manifest: invalid JSON: {e}"))?;

    let schema = v
        .get("schema")
        .and_then(Value::as_i64)
        .ok_or("manifest: missing schema")?;
    if schema != SCHEMA {
        return Err(format!("manifest: unsupported schema {schema}"));
    }
    let channel = v.get("channel").and_then(Value::as_str).unwrap_or("");
    if channel != CHANNEL_STABLE {
        return Err(format!("manifest: unsupported channel {channel:?}"));
    }

    let raw_version = v.get("version").and_then(Value::as_str).unwrap_or("");
    // `v0.7.12` 与 `0.7.12` 都归一化成 `0.7.12`（历史 tag 两种写法都有过）。
    let version = version::normalize_tag(raw_version)
        .ok_or_else(|| format!("manifest: invalid version {raw_version:?}"))?;

    let code = v
        .get("version_code")
        .and_then(Value::as_i64)
        .ok_or("manifest: missing version_code")?;
    if Some(code) != version_code(&version) {
        return Err(format!(
            "manifest: version_code {code} does not match version {version}"
        ));
    }

    let deb = parse_asset(v.pointer("/assets/deb"))?;
    let binary = parse_asset(v.pointer("/assets/binary"))?;
    if deb.is_none() && binary.is_none() {
        return Err("manifest: no usable assets".into());
    }

    let notes_url = v
        .get("notes_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|u| u.starts_with(NOTES_PREFIX))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{TAG_BASE}/{version}"));

    Ok(UpdateManifest {
        version,
        notes_url,
        deb,
        binary,
    })
}

fn parse_asset(v: Option<&Value>) -> Result<Option<ManifestAsset>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("manifest: asset missing name")?;
    // URL 由名字派生：只接受纯文件名，挡掉 `../` 改写路径
    if name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(format!("manifest: illegal asset name {name:?}"));
    }
    let size = v
        .get("size")
        .and_then(Value::as_u64)
        .ok_or("manifest: asset missing size")?;
    let sha = v.get("sha256").and_then(Value::as_str).unwrap_or("").trim();
    if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("manifest: illegal sha256 for {name}"));
    }
    Ok(Some(ManifestAsset {
        name: name.to_string(),
        size,
        sha256: sha.to_ascii_lowercase(),
    }))
}

/// 清单 → 下载资产列表：URL 全部由 `version` 派生，外加 `checksums.txt`
/// （`plan_install` / `install.rs` 都要求 assets 里有它）。
pub fn assets_from(m: &UpdateManifest) -> Vec<UpdateAsset> {
    let base = format!("{ASSET_BASE}/{}", m.version);
    let mut out = Vec::with_capacity(3);
    for a in [m.deb.as_ref(), m.binary.as_ref()].into_iter().flatten() {
        out.push(UpdateAsset {
            name: a.name.clone(),
            url: format!("{base}/{}", a.name),
            size: a.size,
        });
    }
    out.push(UpdateAsset {
        name: CHECKSUMS_NAME.to_string(),
        url: format!("{base}/{CHECKSUMS_NAME}"),
        size: 0,
    });
    out
}

/// 安装前的一致性断言：清单声明必须与 `checksums.txt` 的期望值一致。
///
/// 返回 `Err` 即拒绝安装（fail closed）。清单里没有该资产 → 视为不一致：
/// 能走到下载说明清单本该声明它。
pub fn assert_asset_sha(
    manifest: &UpdateManifest,
    name: &str,
    expected: &str,
) -> Result<(), String> {
    let declared = manifest
        .asset_named(name)
        .ok_or_else(|| format!("manifest: asset {name} is not declared"))?;
    if !declared.sha256.eq_ignore_ascii_case(expected) {
        return Err(format!("manifest: sha256 mismatch for {name}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
