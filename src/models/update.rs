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

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub assets: Vec<UpdateAsset>,
    pub plan: InstallPlan,
}
