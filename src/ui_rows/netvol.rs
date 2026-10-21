//! 网络 / 卷列表行构建（容器计数反推 + 卷体积低频缓存 + 排序）

use slint::SharedString;

use crate::detail_fmt::{dash, format_bytes, hit_any, network_counts};
use crate::models::{NetworkInfo, ResourceSort, VolumeInfo};
use crate::slint_gen::{NetworkRow, VolumeRow};
use crate::ui_state::AppState;

/// 匿名卷展示名截断（prototype：前 12 字符 + …），tooltip 仍给全名
const ANON_KEEP: usize = 12;

pub fn build_network_rows(s: &AppState) -> Vec<NetworkRow> {
    // bollard 的 Network 结构不含连接容器数，由容器列表反推
    let counts = network_counts(s);
    let mut list: Vec<&NetworkInfo> = s
        .networks
        .iter()
        .filter(|n| {
            hit_any(
                &s.container_query,
                &[&n.name, &n.driver, &n.scope, &n.short_id],
            )
        })
        .collect();
    sort_networks(&mut list, s.sort_networks);
    list.into_iter()
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

/// 网络排序（0=名称 1=driver 2=子网）
fn sort_networks(list: &mut [&NetworkInfo], sort: ResourceSort) {
    list.sort_by(|a, b| {
        let cmp = match sort.field {
            1 => a.driver.cmp(&b.driver),
            2 => a.subnet.cmp(&b.subnet),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if sort.desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

pub fn build_volume_rows(s: &AppState) -> Vec<VolumeRow> {
    let mut list: Vec<&VolumeInfo> = s
        .volumes
        .iter()
        .filter(|v| hit_any(&s.container_query, &[&v.name, &v.driver, &v.mountpoint]))
        .collect();
    sort_volumes(&mut list, s.sort_volumes, s);
    list.into_iter()
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

/// 卷排序（0=名称 1=大小 2=挂载点）
///
/// 大小取低频 `/system/df` 缓存；未采样的卷按 0 参与排序（沉到末尾）。
fn sort_volumes(list: &mut [&VolumeInfo], sort: ResourceSort, s: &AppState) {
    list.sort_by(|a, b| {
        let cmp = match sort.field {
            1 => volume_size(s, &a.name).cmp(&volume_size(s, &b.name)),
            2 => a.mountpoint.cmp(&b.mountpoint),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if sort.desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// 卷体积（字节；未采样 = 0）
fn volume_size(s: &AppState, name: &str) -> u64 {
    s.volume_sizes.get(name).copied().unwrap_or(0)
}

fn display_name(name: &str, anonymous: bool) -> String {
    if !anonymous {
        return name.to_string();
    }
    let head: String = name.chars().take(ANON_KEEP).collect();
    format!("{head}…")
}
