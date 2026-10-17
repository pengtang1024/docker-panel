//! 网络 / 卷卡片行构建（容器计数反推 + 卷体积低频缓存）

use slint::SharedString;

use crate::detail_fmt::{dash, format_bytes, hit_any, network_counts};
use crate::slint_gen::{NetworkRow, VolumeRow};
use crate::ui_state::AppState;

/// 匿名卷展示名截断（prototype：前 12 字符 + …），tooltip 仍给全名
const ANON_KEEP: usize = 12;

pub fn build_network_rows(s: &AppState) -> Vec<NetworkRow> {
    // bollard 的 Network 结构不含连接容器数，由容器列表反推
    let counts = network_counts(s);
    s.networks
        .iter()
        .filter(|n| {
            hit_any(
                &s.container_query,
                &[&n.name, &n.driver, &n.scope, &n.short_id],
            )
        })
        .map(|n| NetworkRow {
            id: SharedString::from(n.id.clone()),
            short_id: SharedString::from(n.short_id.clone()),
            name: SharedString::from(n.name.clone()),
            driver: SharedString::from(n.driver.clone()),
            scope: SharedString::from(n.scope.clone()),
            subnet: SharedString::from(n.subnet.clone()),
            containers: *counts.get(n.name.as_str()).unwrap_or(&0) as i32,
            builtin: n.builtin,
        })
        .collect()
}

pub fn build_volume_rows(s: &AppState) -> Vec<VolumeRow> {
    s.volumes
        .iter()
        .filter(|v| {
            hit_any(
                &s.container_query,
                &[&v.name, &v.driver, &v.mountpoint],
            )
        })
        .map(|v| {
            let size = match s.volume_sizes.get(&v.name) {
                Some(bytes) => format_bytes(*bytes),
                None => dash(),
            };
            VolumeRow {
                name: SharedString::from(v.name.clone()),
                display_name: SharedString::from(display_name(&v.name, v.anonymous)),
                driver: SharedString::from(v.driver.clone()),
                mountpoint: SharedString::from(v.mountpoint.clone()),
                size: SharedString::from(size),
                anonymous: v.anonymous,
            }
        })
        .collect()
}

fn display_name(name: &str, anonymous: bool) -> String {
    if anonymous == false {
        return name.to_string();
    }
    let head: String = name.chars().take(ANON_KEEP).collect();
    format!("{head}…")
}
