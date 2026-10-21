//! Slint 行构建 —— 从 AppState 数据生成 Slint Model 行
//!
//! 拆分理由：Compose 分组排序、端口压缩、相对时间等格式化逻辑较重，
//! 全部堆在一个文件会超过 300 行上限。
//!
//!   containers.rs   → 容器台账行（筛选 / 排序 / 归一化比例）
//!   ledger.rs       → 台账概览带 + 台账入口
//!   compose_tree.rs → 台账内联树行（项目带 / 服务带 + 容器行的混合列表）
//!   images.rs       → 镜像卡片行
//!   netvol.rs       → 网络 / 卷卡片行

mod compose_tree;
mod containers;
mod images;
mod ledger;
mod netvol;

pub use images::build_image_rows;
pub use ledger::build_container_ledger;
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
        0x38bdf8, 0xa78bfa, 0x34d399, 0xfbbf24, 0xf472b6, 0x60a5fa, 0xfb923c, 0x4ade80,
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
