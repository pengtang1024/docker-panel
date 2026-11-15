//! 自动更新的数据类型（models 层，不依赖 update 运行时）。
//!
//! `Msg` / `AppState` / `update` 共用这些类型；把它们放在 models 层
//! 可以保持依赖方向 `update → models`，避免 models 反向依赖 update。

#[derive(Debug, Clone)]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Deb,
    Binary,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    DevBuild,
    Platform,
    NoPkexec,
    NoDeb,
    NoBinary,
    NoChecksums,
    NotWritable,
}

impl UnsupportedReason {
    /// msgid（由 UI 侧 `i18n::t` 翻译）。
    pub fn msgid(self) -> &'static str {
        match self {
            Self::DevBuild => "Development builds do not install updates automatically",
            Self::Platform => "Automatic install is not supported on this platform",
            Self::NoPkexec => "pkexec is required to install the deb package",
            Self::NoDeb => "No deb package is available for this architecture",
            Self::NoBinary => "The release does not include a binary asset",
            Self::NoChecksums => "The release does not include checksums.txt",
            Self::NotWritable => {
                "The install directory is not writable, please use the download page"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub kind: InstallKind,
    pub asset: Option<UpdateAsset>,
    pub reason: Option<UnsupportedReason>,
}

impl InstallPlan {
    pub fn installable(&self) -> bool {
        self.kind != InstallKind::Unsupported && self.asset.is_some()
    }
}

/// 静态清单（`version.json`）里的单个资产声明。
///
/// **不可信**：`name` 用于派生下载 URL，`sha256` / `size` 只能用来与 Release 的
/// `checksums.txt` 交叉核对，绝不能替代它（见 docs/arch/update-manifest.md §7）。
#[derive(Debug, Clone)]
pub struct ManifestAsset {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// 静态更新清单。只当「版本号广播」用：资产 URL 由 `version` 派生，
/// 下载完整性仍由 `checksums.txt` 决定。
#[derive(Debug, Clone)]
pub struct UpdateManifest {
    pub version: String,
    pub notes_url: String,
    pub deb: Option<ManifestAsset>,
    pub binary: Option<ManifestAsset>,
}

impl UpdateManifest {
    /// 按资产名找清单里的声明（deb / 裸二进制）。
    pub fn asset_named(&self, name: &str) -> Option<&ManifestAsset> {
        [self.deb.as_ref(), self.binary.as_ref()]
            .into_iter()
            .flatten()
            .find(|a| a.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub assets: Vec<UpdateAsset>,
    pub plan: InstallPlan,
    /// 来源是静态清单时保留原始声明，安装阶段用它做一致性断言。
    pub manifest: Option<UpdateManifest>,
}
