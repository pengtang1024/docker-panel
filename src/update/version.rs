//! 更新相关的纯逻辑：版本比较 / 资产选择 / 校验和解析 / 更新说明截断。
//!
//! 本模块**不做任何 IO**（不联网、不读文件、不执行命令），所有分支都可以单测。
//! 环境探测与下载安装见 `install.rs`。

use crate::update::{InstallEnv, InstallKind, InstallPlan, UnsupportedReason, UpdateAsset};

/// 把 `v0.6.0` / `0.6.0` 归一化成 `0.6.0`；非法版本返回 None。
pub fn normalize_tag(tag: &str) -> Option<String> {
    let t = tag.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let (major, minor, patch) = parse_version(t)?;
    Some(format!("{major}.{minor}.{patch}"))
}

/// 严格解析 `major.minor.patch`（每段 u64，禁止预发布后缀）。
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 严格大于才返回 true。两个版本都必须是合法语义版本；
/// 解析失败一律返回 false（宁可漏提示，也不进入循环提示）。
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// 当前平台在 release 资产名里使用的架构后缀（GitHub 构建为 amd64 / arm64）。
pub fn target_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => "",
    }
}

/// 从 release 资产里挑出 deb / 裸二进制 / checksums。
pub fn select_asset(assets: &[UpdateAsset], kind: InstallKind, arch: &str) -> Option<UpdateAsset> {
    match kind {
        InstallKind::Deb => assets
            .iter()
            .find(|a| {
                a.name.starts_with("docker-panel_")
                    && !arch.is_empty()
                    && a.name.ends_with(&format!("_{arch}.deb"))
            })
            .cloned(),
        InstallKind::Binary => assets.iter().find(|a| a.name == "docker-panel").cloned(),
        InstallKind::Unsupported => None,
    }
}

/// 根据本机环境决定安装方式。纯函数，便于覆盖全部分支。
pub fn plan_install(assets: &[UpdateAsset], env: &InstallEnv) -> InstallPlan {
    let unsupported = |reason| InstallPlan {
        kind: InstallKind::Unsupported,
        asset: None,
        reason: Some(reason),
    };

    if env.debug_build {
        return unsupported(UnsupportedReason::DevBuild);
    }
    if env.os != "linux" {
        return unsupported(UnsupportedReason::Platform);
    }

    if env.dpkg_managed {
        if !env.pkexec_available {
            return unsupported(UnsupportedReason::NoPkexec);
        }
        return match select_asset(assets, InstallKind::Deb, env.arch) {
            Some(asset) if has_checksums(assets) => InstallPlan {
                kind: InstallKind::Deb,
                asset: Some(asset),
                reason: None,
            },
            Some(_) => unsupported(UnsupportedReason::NoChecksums),
            None => unsupported(UnsupportedReason::NoDeb),
        };
    }

    if !env.exe_dir_writable {
        return unsupported(UnsupportedReason::NotWritable);
    }
    match select_asset(assets, InstallKind::Binary, env.arch) {
        Some(asset) if has_checksums(assets) => InstallPlan {
            kind: InstallKind::Binary,
            asset: Some(asset),
            reason: None,
        },
        Some(_) => unsupported(UnsupportedReason::NoChecksums),
        None => unsupported(UnsupportedReason::NoBinary),
    }
}

fn has_checksums(assets: &[UpdateAsset]) -> bool {
    assets.iter().any(|a| a.name == "checksums.txt")
}

/// 从 `checksums.txt`（sha256sum 格式）里找目标文件的 SHA-256。
///
/// 兼容 `hash  name` 与 `hash *name`（binary 模式），大小写不敏感。
pub fn checksum_for(checksums: &str, asset_name: &str) -> Option<String> {
    for line in checksums.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?;
        let name = name.trim_start_matches('*');
        if name != asset_name {
            continue;
        }
        if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hash.to_ascii_lowercase());
        }
        return None;
    }
    None
}

/// 更新说明截断：按字符边界，避免把一个 UTF-8 字符劈成两半。
pub fn truncate_notes(notes: &str, max_chars: usize) -> String {
    let normalized = notes.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> UpdateAsset {
        UpdateAsset {
            name: name.into(),
            url: format!("https://example.invalid/{name}"),
            size: 1,
        }
    }

    fn env() -> InstallEnv {
        InstallEnv {
            debug_build: false,
            os: "linux",
            arch: "amd64",
            dpkg_managed: true,
            pkexec_available: true,
            exe_dir_writable: false,
        }
    }

    #[test]
    fn version_normalization_and_compare() {
        assert_eq!(normalize_tag("v0.6.0").as_deref(), Some("0.6.0"));
        assert_eq!(normalize_tag("0.6.0").as_deref(), Some("0.6.0"));
        assert_eq!(normalize_tag("v1.2"), None);
        assert!(is_newer("0.6.0", "0.5.0"));
        assert!(!is_newer("0.5.0", "0.5.0"));
        assert!(!is_newer("0.4.9", "0.5.0"));
        assert!(!is_newer("garbage", "0.5.0"));
    }

    #[test]
    fn selects_arch_specific_deb() {
        let assets = vec![
            asset("docker-panel_0.6.0_arm64.deb"),
            asset("docker-panel_0.6.0_amd64.deb"),
            asset("checksums.txt"),
        ];
        let picked = select_asset(&assets, InstallKind::Deb, "amd64").unwrap();
        assert_eq!(picked.name, "docker-panel_0.6.0_amd64.deb");
        assert!(select_asset(&assets, InstallKind::Deb, "riscv64").is_none());
    }

    #[test]
    fn selects_raw_binary_only_by_exact_name() {
        let assets = vec![asset("docker-panel"), asset("docker-panel_0.6.0_amd64.deb")];
        assert_eq!(
            select_asset(&assets, InstallKind::Binary, "amd64")
                .unwrap()
                .name,
            "docker-panel"
        );
    }

    #[test]
    fn parses_sha256_lines() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let text = format!("{a}  docker-panel_0.6.0_amd64.deb\r\n{b} *docker-panel\r\nshort bad\n");
        assert_eq!(
            checksum_for(&text, "docker-panel_0.6.0_amd64.deb").as_deref(),
            Some(a.as_str())
        );
        assert_eq!(
            checksum_for(&text, "docker-panel").as_deref(),
            Some(b.to_ascii_lowercase().as_str())
        );
        assert_eq!(checksum_for(&text, "missing"), None);
        assert_eq!(checksum_for("zz", "x"), None);
    }

    #[test]
    fn truncates_notes_on_char_boundary() {
        assert_eq!(truncate_notes("hello\r\nworld", 100), "hello\nworld");
        assert_eq!(truncate_notes("abcdef", 3), "abc…");
        assert_eq!(truncate_notes("中文说明", 2), "中文…");
    }

    #[test]
    fn plan_prefers_deb_when_dpkg_managed() {
        let assets = vec![
            asset("docker-panel_0.6.0_amd64.deb"),
            asset("checksums.txt"),
        ];
        let p = plan_install(&assets, &env());
        assert_eq!(p.kind, InstallKind::Deb);
        assert_eq!(p.asset.unwrap().name, "docker-panel_0.6.0_amd64.deb");
    }

    #[test]
    fn plan_rejects_unsupported_environments() {
        let assets = vec![asset("docker-panel_0.6.0_amd64.deb")];

        let mut e = env();
        e.debug_build = true;
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::DevBuild)
        );

        let mut e = env();
        e.os = "macos";
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::Platform)
        );

        let mut e = env();
        e.pkexec_available = false;
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::NoPkexec)
        );

        let mut e = env();
        e.arch = "riscv64";
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::NoDeb)
        );

        let mut e = env();
        e.dpkg_managed = false;
        e.exe_dir_writable = false;
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::NotWritable)
        );

        let mut e = env();
        e.dpkg_managed = false;
        e.exe_dir_writable = true;
        assert_eq!(
            plan_install(&assets, &e).reason,
            Some(UnsupportedReason::NoBinary)
        );

        let no_sum = vec![asset("docker-panel_0.6.0_amd64.deb")];
        let e = env();
        assert_eq!(
            plan_install(&no_sum, &e).reason,
            Some(UnsupportedReason::NoChecksums)
        );
    }

    #[test]
    fn plan_uses_binary_for_writable_non_dpkg_install() {
        let assets = vec![asset("docker-panel"), asset("checksums.txt")];
        let mut e = env();
        e.dpkg_managed = false;
        e.exe_dir_writable = true;
        let p = plan_install(&assets, &e);
        assert_eq!(p.kind, InstallKind::Binary);
        assert_eq!(p.asset.unwrap().name, "docker-panel");
    }
}
