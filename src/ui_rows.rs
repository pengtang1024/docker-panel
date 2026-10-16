//! Slint 行构建 —— 从 AppState 数据生成 Slint Model 行
//!
//! 这是独立模块，因为 Compose 分组排序 + 格式化逻辑较重，
//! 单独放避免 main.rs 膨胀（main.rs 必须 ≤300 行）。

use std::collections::HashMap;

use slint::SharedString;

use crate::models::{ContainerInfo, SortField};
use crate::slint_gen::{ContainerRow, ImageRow, NetworkRow, VolumeRow};
use crate::ui_state::{format_bytes, AppState};

pub fn build_container_rows(s: &AppState) -> Vec<ContainerRow> {
    let mut filtered: Vec<&ContainerInfo> = s
        .containers
        .iter()
        .filter(|c| {
            s.container_query.is_empty() || {
                let q = s.container_query.to_lowercase();
                c.name.to_lowercase().contains(&q)
                    || c.short_id().contains(&q)
                    || c.image.to_lowercase().contains(&q)
                    || c.state.to_lowercase().contains(&q)
                    || c.compose_project
                        .as_deref()
                        .map(|p| p.to_lowercase().contains(&q))
                        .unwrap_or(false)
                    || c.compose_service
                        .as_deref()
                        .map(|p| p.to_lowercase().contains(&q))
                        .unwrap_or(false)
                    || c.ports.to_lowercase().contains(&q)
            }
        })
        .collect();

    // —— 排序 ——
    match s.sort_field {
        SortField::Group => {
            // Compose 分组排序（默认）
            filtered.sort_by(|a, b| {
                let ap = a.compose_project.as_deref().unwrap_or("~");
                let bp = b.compose_project.as_deref().unwrap_or("~");
                let cmp = ap.cmp(bp).then_with(|| {
                    a.compose_service
                        .as_deref()
                        .cmp(&b.compose_service.as_deref())
                });
                if s.sort_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Name => {
            filtered.sort_by(|a, b| {
                let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if s.sort_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Status => {
            filtered.sort_by(|a, b| {
                // running 在前，按 name 次级
                let ar = a.is_running() as u8;
                let br = b.is_running() as u8;
                let cmp = br.cmp(&ar).then_with(|| a.name.cmp(&b.name));
                if s.sort_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Cpu => {
            filtered.sort_by(|a, b| {
                let cmp = b
                    .cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal);
                if s.sort_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Mem => {
            filtered.sort_by(|a, b| {
                let cmp = b.mem_usage.cmp(&a.mem_usage);
                if s.sort_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
    }

    // —— 只在 Group 排序下生成分组行 ——
    let grouping = matches!(s.sort_field, SortField::Group);
    let mut last: Option<&str> = None;
    filtered
        .into_iter()
        .map(|c| {
            let project = c.compose_project.as_deref();
            let is_group = grouping
                && match (project, last) {
                    (Some(_), None) | (None, Some(_)) => true,
                    (Some(a), Some(b)) => a != b,
                    _ => false,
                };
            if grouping {
                last = project;
            }
            ContainerRow {
                id: SharedString::from(c.id.clone()),
                short_id: SharedString::from(c.short_id()),
                name: SharedString::from(c.name.clone()),
                image: SharedString::from(c.image.clone()),
                status: SharedString::from(c.status.clone()),
                is_running: c.is_running(),
                compose_project: SharedString::from(c.compose_project.clone().unwrap_or_default()),
                compose_service: SharedString::from(c.compose_service.clone().unwrap_or_default()),
                is_group_start: is_group,
                cpu_percent: if c.cpu_percent > 0.0 {
                    SharedString::from(format!("{:.1}%", c.cpu_percent))
                } else {
                    SharedString::default()
                },
                mem_usage: if c.mem_usage > 0 && c.mem_limit > 0 {
                    SharedString::from(format!(
                        "{} / {}",
                        format_bytes(c.mem_usage),
                        format_bytes(c.mem_limit)
                    ))
                } else if c.mem_usage > 0 {
                    SharedString::from(format_bytes(c.mem_usage))
                } else {
                    SharedString::default()
                },
                net_traffic: if c.net_rx > 0 || c.net_tx > 0 {
                    SharedString::from(format!(
                        "↓{} ↑{}",
                        format_bytes(c.net_rx),
                        format_bytes(c.net_tx)
                    ))
                } else {
                    SharedString::default()
                },
            }
        })
        .collect()
}

pub fn build_image_rows(s: &AppState) -> Vec<ImageRow> {
    s.images
        .iter()
        .filter(|img| {
            s.image_query.is_empty() || {
                let q = s.image_query.to_lowercase();
                img.name.to_lowercase().contains(&q)
                    || img.tag.to_lowercase().contains(&q)
                    || img.short_id().contains(&q)
            }
        })
        .map(|img| ImageRow {
            id: SharedString::from(img.key()),
            short_id: SharedString::from(img.short_id()),
            name: SharedString::from(img.name.clone()),
            tag: SharedString::from(img.tag.clone()),
            size: SharedString::from(format_bytes(img.size as u64)),
            containers: img.containers as i32,
        })
        .collect()
}

pub fn build_network_rows(s: &AppState) -> Vec<NetworkRow> {
    // 从容器列表反推每个 network 的连接数（bollard Network struct 本身没有此字段）
    let mut net_counts: HashMap<&str, usize> = HashMap::new();
    for c in &s.containers {
        for net in &c.networks {
            *net_counts.entry(net.as_str()).or_insert(0) += 1;
        }
    }
    s.networks
        .iter()
        .map(|n| NetworkRow {
            id: SharedString::from(n.id.clone()),
            short_id: SharedString::from(n.short_id.clone()),
            name: SharedString::from(n.name.clone()),
            driver: SharedString::from(n.driver.clone()),
            scope: SharedString::from(n.scope.clone()),
            containers: *net_counts.get(n.name.as_str()).unwrap_or(&0) as i32,
        })
        .collect()
}

pub fn build_volume_rows(s: &AppState) -> Vec<VolumeRow> {
    s.volumes
        .iter()
        .map(|v| VolumeRow {
            name: SharedString::from(v.name.clone()),
            driver: SharedString::from(v.driver.clone()),
            mountpoint: SharedString::from(v.mountpoint.clone()),
            size: SharedString::from(v.size.clone()),
        })
        .collect()
}
