//! Slint 行构建 —— 从 AppState 数据生成 Slint Model 行
//!
//! 拆分理由：Compose 分组排序、端口压缩、相对时间等格式化逻辑较重，
//! 全部堆在一个文件会超过 300 行上限。
//!
//!   containers.rs → 容器卡片行
//!   images.rs     → 镜像卡片行
//!   netvol.rs     → 网络 / 卷卡片行

mod containers;
mod images;
mod netvol;

use std::collections::BTreeMap;

use slint::SharedString;

use crate::slint_gen::ComposeRow;
use crate::ui_state::AppState;

pub use containers::build_container_rows;
pub use images::build_image_rows;
pub use netvol::{build_network_rows, build_volume_rows};

/// 卡片端口压缩：去掉 "ip:" 前缀、`(tcp)` 归一为 `/tcp`
/// "0.0.0.0:6399->6379(tcp)" → "6399→6379/tcp"；"6379(tcp)" → "6379/tcp"
pub(crate) fn compact_port(p: &str) -> String {
    let mut s = p.to_string();
    if let Some(i) = s.rfind('(') {
        let proto = s[i + 1..].trim_end_matches(')').to_string();
        s.truncate(i);
        s.push('/');
        s.push_str(&proto);
    }
    let s = match s.find("->") {
        Some(arrow) => match s[..arrow].rfind(':') {
            Some(ci) => format!("{}{}", &s[ci + 1..], &s[arrow..]),
            None => s,
        },
        None => s,
    };
    s.replace("->", "→")
}

/// 卡片芯片短标签：只保留对外可达端口（有映射取映射前段），去掉协议
/// "6399→6379/tcp" → "6399"；"6379/tcp" → "6379"
pub(crate) fn short_port(compact: &str) -> &str {
    let no_proto = compact.split('/').next().unwrap_or(compact);
    no_proto.split('→').next().unwrap_or(no_proto)
}

/// Compose 项目名 → 稳定散列配色（FNV-1a）
pub(crate) fn proj_color(name: &str) -> slint::Color {
    const PALETTE: [u32; 8] = [
        0x38bd_f8, 0xa78b_fa, 0x34d3_99, 0xfbbf_24, 0xf472_b6, 0x60a5_fa, 0xfb92_3c, 0x4ade_80,
    ];
    if name.is_empty() {
        return slint::Color::from_rgb_u8(0xa7, 0x8b, 0xfa);
    }
    let mut h: u32 = 2_166_136_261;
    for b in name.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
    }
    let c = PALETTE[h as usize % PALETTE.len()];
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// SubBar 的 Compose 项目筛选芯片（按项目名去重、计数、稳定配色、名称升序）
pub fn build_compose_rows(s: &AppState) -> Vec<ComposeRow> {
    let mut counts: BTreeMap<String, i32> = BTreeMap::new();
    for c in &s.containers {
        if let Some(p) = c.compose_project.as_deref() {
            if !p.is_empty() {
                *counts.entry(p.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|(name, n)| ComposeRow {
            color: proj_color(&name),
            name: SharedString::from(name),
            n,
        })
        .collect()
}
