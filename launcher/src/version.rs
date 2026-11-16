//! 版本号解析与比较：只服务「用户副本 / deb 自带种子」之间的新旧判定。
//!
//! 启动器不需要完整 semver：只需要在 `0.7.18` / `0.7.18-rc.1` 这类发行标签上判断谁更新。
//! 规则：先比 major.minor.patch，再让预发布版低于同号正式版（`1.0.0-rc.1 < 1.0.0`）。

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    /// 带 `-` 后缀（rc/beta/…）时为 true，排序低于同号正式版。
    prerelease: bool,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        // 丢掉构建元数据（`+abc`），它的存在不影响新旧
        let text = text.split('+').next().unwrap_or(text);
        let (core, prerelease) = match text.split_once('-') {
            Some((core, _)) => (core, true),
            None => (text, false),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        // 四段及以上（1.2.3.4）不是本项目的发行格式，直接当解析失败，避免误判谁更新
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if self.prerelease {
            // 只用于日志：原始后缀（rc.1 之类）没有保留，不影响排序
            write!(f, "-pre")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            // 正式版(prerelease=false) 高于预发布版(true)，所以比较方向要反过来
            .then_with(|| other.prerelease.cmp(&self.prerelease))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("版本号应可解析")
    }

    #[test]
    fn parses_release_tag() {
        assert_eq!(v("0.7.18").cmp(&v("0.7.18")), std::cmp::Ordering::Equal);
        assert!(v("0.7.19") > v("0.7.18"));
        assert!(v("0.10.0") > v("0.9.9"));
    }

    #[test]
    fn ignores_prefix_and_build_metadata() {
        assert_eq!(v("v0.7.18"), v("0.7.18"));
        assert_eq!(v("0.7.18+build.5"), v("0.7.18"));
    }

    #[test]
    fn prerelease_is_older_than_release() {
        assert!(v("0.8.0-rc.1") < v("0.8.0"));
        assert!(v("0.8.0-rc.1") > v("0.7.18"));
    }

    #[test]
    fn rejects_non_release_shapes() {
        assert!(Version::parse("").is_none());
        assert!(Version::parse("0.7").is_none());
        assert!(Version::parse("0.7.18.1").is_none());
        assert!(Version::parse("0.x.18").is_none());
    }
}
