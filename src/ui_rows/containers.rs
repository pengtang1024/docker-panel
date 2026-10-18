//! 容器卡片行构建（Compose 分组排序 + 端口压缩 + 资源格式化）

use slint::SharedString;

use crate::detail_fmt::{format_bytes, format_bytes_compact};
use crate::models::{ContainerInfo, SortField};
use crate::slint_gen::ContainerRow;
use crate::ui_state::AppState;

use super::{compact_port, proj_color, short_port};

pub fn build_container_rows(s: &AppState) -> Vec<ContainerRow> {
    let mut filtered: Vec<&ContainerInfo> = s
        .containers
        .iter()
        .filter(|c| {
            // 状态 / Compose 项目筛选（芯片 key：all / running / stopped / <项目名>）
            match s.filter.as_str() {
                "" | "all" => {}
                "running" => {
                    if !c.is_running() {
                        return false;
                    }
                }
                "stopped" => {
                    if c.is_running() {
                        return false;
                    }
                }
                project => {
                    if c.compose_project.as_deref() != Some(project) {
                        return false;
                    }
                }
            }
            if s.container_query.is_empty() {
                return true;
            }
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
        })
        .collect();

    // —— 排序 ——
    match s.sort_field {
        SortField::Group => {
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
        SortField::Name => filtered.sort_by(|a, b| {
            let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
            if s.sort_desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Status => filtered.sort_by(|a, b| {
            // running 在前，按 name 次级
            let cmp = (b.is_running() as u8)
                .cmp(&(a.is_running() as u8))
                .then_with(|| a.name.cmp(&b.name));
            if s.sort_desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Cpu => filtered.sort_by(|a, b| {
            let cmp = b
                .cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal);
            if s.sort_desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Mem => filtered.sort_by(|a, b| {
            let cmp = b.mem_usage.cmp(&a.mem_usage);
            if s.sort_desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Disk => filtered.sort_by(|a, b| {
            let cmp = (b.block_read + b.block_write).cmp(&(a.block_read + a.block_write));
            if s.sort_desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
    }

    // —— 只在 Group 排序下生成分组行 ——
    let grouping = matches!(s.sort_field, SortField::Group);
    let mut last: Option<&str> = None;
    filtered
        .into_iter()
        .map(|c| {
            // 分组比较始终用英文 key，显示时再走 i18n 翻译
            let project = c.compose_project.as_deref().or(Some("Standalone"));
            let is_group = grouping
                && match (project, last) {
                    (Some(a), Some(b)) => a != b,
                    _ => last.is_none(),
                };
            if grouping {
                last = project;
            }
            // 端口：卡片最多展示 3 条（芯片只放短标签，避免长映射串溢出卡片宽），
            // 其余走 "+N" 角标；芯片与角标的 tooltip 都列全部端口全文
            let all_ports: Vec<String> = if c.ports.trim().is_empty() {
                Vec::new()
            } else {
                c.ports
                    .split(", ")
                    .filter(|p| !p.is_empty())
                    .map(compact_port)
                    .collect()
            };
            let ports_text: String = all_ports.join(", ");
            let shown: Vec<&str> = all_ports.iter().map(|p| short_port(p)).take(3).collect();
            ContainerRow {
                id: SharedString::from(c.id.clone()),
                short_id: SharedString::from(c.short_id()),
                name: SharedString::from(c.name.clone()),
                image: SharedString::from(c.image.clone()),
                status: SharedString::from(c.status.clone()),
                is_running: c.is_running(),
                is_selected: s.batch.contains(&c.id),
                // 非 compose 容器置空（spec 3.1：proj-tag 只在真 compose 项目时显示）
                compose_project: SharedString::from(c.compose_project.clone().unwrap_or_default()),
                compose_color: proj_color(c.compose_project.as_deref().unwrap_or_default()),
                compose_service: SharedString::from(c.compose_service.clone().unwrap_or_default()),
                is_group_start: is_group,
                // prototype .stat 数值风格：整数百分比（"3%"）+ 紧凑字节（"8M"）
                cpu_percent: if c.cpu_percent > 0.0 {
                    SharedString::from(format!("{:.0}%", c.cpu_percent))
                } else {
                    SharedString::default()
                },
                mem_usage: if c.mem_usage > 0 {
                    SharedString::from(format_bytes_compact(c.mem_usage))
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
                disk: SharedString::from(format_bytes_compact(c.size_rw)),
                port1: SharedString::from(shown.first().copied().unwrap_or_default()),
                port2: SharedString::from(shown.get(1).copied().unwrap_or_default()),
                port3: SharedString::from(shown.get(2).copied().unwrap_or_default()),
                ports_more: (all_ports.len() as i32 - 3).max(0),
                ports_text: SharedString::from(ports_text),
            }
        })
        .collect()
}
