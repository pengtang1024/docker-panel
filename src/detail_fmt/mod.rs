//! 抽屉内容格式化：AppState → [DetailPair] 行列表
//!
//! 同一个抽屉要展示容器 / 镜像 / 网络 / 卷四种结构，固定字段属性无法表达，
//! 统一由本模块按 `AppState.sel` 生成行列表（对齐 ui/prototype.html 的
//! detailHTML / envHTML）。
//!
//! 分层：
//!   container.rs → 容器详情 5 节 + 环境 4 节
//!   image.rs     → 镜像详情 2 节
//!   netvol.rs    → 网络 / 卷详情

mod container;
mod image;
mod netvol;

use std::collections::HashMap;

use slint::SharedString;

use crate::models::ContainerInfo;
use crate::slint_gen::DetailPair;
use crate::ui_state::{AppState, SelKind};

/// 详情行类型（与 models.slint DetailPair.kind 对应）
pub const K_KV: i32 = 0;
pub const K_HEAD: i32 = 1;
pub const K_BOX: i32 = 2;
pub const K_CHIP: i32 = 3;

/// 抽屉一次打开所需的全部内容
pub struct DrawerData {
    pub title: String,
    pub sub: String,
    /// 仅容器展示 Tab 栏
    pub tabs_visible: bool,
    pub detail: Vec<DetailPair>,
    pub env: Vec<DetailPair>,
}

/// 把字节数格式化为 "12.3 MB" 形式
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// 卡片紧凑字节格式（prototype .stat b 风格：8M / 512K / 1.2G，无空格）
pub fn format_bytes_compact(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{}K", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{}M", bytes / (1024 * 1024))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// unix 秒 → 相对时间文本（"2 days ago"）
pub fn rel_time(secs: i64) -> String {
    if secs <= 0 {
        return dash();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let d = (now - secs).max(0);
    let (n, key) = if d < 3600 {
        ((d / 60).max(1), "{0} minutes ago")
    } else if d < 86_400 {
        (d / 3600, "{0} hours ago")
    } else if d < 7 * 86_400 {
        (d / 86_400, "{0} days ago")
    } else if d < 30 * 86_400 {
        (d / (7 * 86_400), "{0} weeks ago")
    } else {
        (d / (30 * 86_400), "{0} months ago")
    };
    crate::i18n::trf(key, &[&n.to_string()])
}

/// 空值统一占位
pub fn dash() -> String {
    "—".into()
}

/// 空串 / 空列表统一占位，否则原样返回
pub(crate) fn or_dash(s: &str) -> String {
    if s.trim().is_empty() {
        dash()
    } else {
        s.to_string()
    }
}

pub(crate) fn join_or(list: &[String], sep: &str) -> String {
    if list.is_empty() {
        dash()
    } else {
        list.join(sep)
    }
}

pub(crate) fn head(label: &str) -> DetailPair {
    DetailPair {
        kind: K_HEAD,
        k: SharedString::from(label),
        v: SharedString::default(),
        mono: false,
    }
}

pub(crate) fn kv(k: &str, v: String, mono: bool) -> DetailPair {
    DetailPair {
        kind: K_KV,
        k: SharedString::from(k),
        v: SharedString::from(v),
        mono,
    }
}

/// k 为空 → 整宽代码盒（prototype 的独立 .box）
pub(crate) fn kbox(k: &str, v: String) -> DetailPair {
    DetailPair {
        kind: K_BOX,
        k: SharedString::from(k),
        v: SharedString::from(v),
        mono: true,
    }
}

/// 代码盒条目；k 非空时以高亮键名 + "=" 前缀渲染
pub(crate) fn chip(k: &str, v: String) -> DetailPair {
    DetailPair {
        kind: K_CHIP,
        k: SharedString::from(k),
        v: SharedString::from(v),
        mono: true,
    }
}

/// 按当前选中类型生成抽屉内容
pub fn build_drawer_data(s: &AppState) -> Option<DrawerData> {
    match s.sel {
        SelKind::Container => container::build(s),
        SelKind::Image => image::build(s),
        SelKind::Network => netvol::build_network(s),
        SelKind::Volume => netvol::build_volume(s),
    }
}

/// 某网络下挂载的容器数（bollard 的 Network 结构不含此字段，由容器列表反推）
pub fn network_container_count(s: &AppState, name: &str) -> usize {
    s.containers
        .iter()
        .filter(|c| c.networks.iter().any(|n| n == name))
        .count()
}

/// 全部网络的连接数映射（卡片网格一次遍历算完，避免逐行 O(n²)）
pub(crate) fn network_counts(s: &AppState) -> HashMap<&str, usize> {
    let mut m: HashMap<&str, usize> = HashMap::new();
    for c in &s.containers {
        for n in &c.networks {
            *m.entry(n.as_str()).or_insert(0) += 1;
        }
    }
    m
}

/// 容器/镜像的检索匹配（容器 + 镜像 + 网络/卷三处共用同一段查询词）
pub(crate) fn hit_any(q: &str, fields: &[&str]) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    fields.iter().any(|f| f.to_lowercase().contains(&q))
}

pub(crate) fn container_of<'a>(s: &'a AppState, id: &str) -> Option<&'a ContainerInfo> {
    s.containers.iter().find(|c| c.id == id)
}
