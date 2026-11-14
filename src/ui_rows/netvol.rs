//! 网络 / 卷列表行构建（容器计数反推 + 卷体积低频缓存 + 排序）

use slint::SharedString;

use crate::detail_fmt::{dash, hit_any, network_counts};
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
                Some(bytes) => crate::format::format_bytes(*bytes),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn network(name: &str, driver: &str, subnet: &str) -> NetworkInfo {
        NetworkInfo {
            id: format!("id-{name}"),
            short_id: name.to_string(),
            name: name.to_string(),
            driver: driver.to_string(),
            scope: "local".to_string(),
            subnet: subnet.to_string(),
            gateway: String::new(),
            builtin: false,
        }
    }

    fn volume(name: &str, mountpoint: &str, anonymous: bool) -> VolumeInfo {
        VolumeInfo {
            name: name.to_string(),
            driver: "local".to_string(),
            mountpoint: mountpoint.to_string(),
            created: String::new(),
            anonymous,
        }
    }

    fn net_names(sorted: &[&NetworkInfo]) -> Vec<String> {
        sorted.iter().map(|n| n.name.clone()).collect()
    }

    /// 网络列：名称忽略大小写，driver / 子网按字符串升序，desc 只负责反向
    #[test]
    fn network_sort_follows_column_and_direction() {
        let list = [
            network("mid", "bridge", "172.18.0.0/16"),
            network("Alpha", "host", "172.17.0.0/16"),
            network("zeta", "overlay", "10.0.0.0/8"),
        ];

        let mut refs: Vec<&NetworkInfo> = list.iter().collect();
        sort_networks(
            &mut refs,
            ResourceSort {
                field: 0,
                desc: false,
            },
        );
        assert_eq!(net_names(&refs), vec!["Alpha", "mid", "zeta"]);

        let mut refs: Vec<&NetworkInfo> = list.iter().collect();
        sort_networks(
            &mut refs,
            ResourceSort {
                field: 1,
                desc: false,
            },
        );
        assert_eq!(net_names(&refs), vec!["mid", "Alpha", "zeta"]);

        let mut refs: Vec<&NetworkInfo> = list.iter().collect();
        sort_networks(
            &mut refs,
            ResourceSort {
                field: 2,
                desc: true,
            },
        );
        assert_eq!(net_names(&refs), vec!["mid", "Alpha", "zeta"]);
    }

    fn vol_names(sorted: &[&VolumeInfo]) -> Vec<String> {
        sorted.iter().map(|v| v.name.clone()).collect()
    }

    /// 卷大小取 `/system/df` 缓存；未采样的卷按 0 参与排序
    #[test]
    fn volume_sort_uses_cached_sizes() {
        let (state, _rx) = AppState::new_test();
        let mut s = state.lock().unwrap();
        s.volume_sizes.insert("big".into(), 900);
        s.volume_sizes.insert("mid".into(), 500);
        // small 未采样 → 0

        let list = [
            volume("mid", "/var/lib/docker/volumes/mid", false),
            volume("big", "/var/lib/docker/volumes/big", false),
            volume("small", "/var/lib/docker/volumes/small", false),
        ];
        let mut refs: Vec<&VolumeInfo> = list.iter().collect();
        sort_volumes(
            &mut refs,
            ResourceSort {
                field: 1,
                desc: false,
            },
            &s,
        );
        assert_eq!(vol_names(&refs), vec!["small", "mid", "big"]);

        let mut refs: Vec<&VolumeInfo> = list.iter().collect();
        sort_volumes(
            &mut refs,
            ResourceSort {
                field: 2,
                desc: false,
            },
            &s,
        );
        assert_eq!(
            vol_names(&refs),
            vec!["big", "mid", "small"],
            "挂载点字符串升序"
        );
    }

    /// 匿名卷展示名截断到 12 字符 + 省略号；具名卷原样返回
    #[test]
    fn anonymous_volume_name_is_truncated() {
        let long = "0123456789abcdef0123456789abcdef";
        assert_eq!(display_name(long, true), "0123456789ab…");
        assert_eq!(display_name("my-data", false), "my-data");
        assert_eq!(display_name("my-data", true), "my-data…");
    }
}
