//! 容器台账行构建（筛选 + 排序 + 归一化资源比例 + 异常标记）
//!
//! 台账形态（方向 A）：一行一个容器，列起点固定、数值右对齐等宽体，
//! CPU / 内存用微条表示，**长度按当前视图内的最大值归一**（列内可比，
//! 不是绝对值占比）——这样 12.4% 和 87.4% 一眼可分。

use slint::SharedString;

use crate::detail_fmt::format_bytes_compact;
use crate::models::{ContainerInfo, SortField};
use crate::slint_gen::ContainerRow;
use crate::ui_state::AppState;

use super::{compact_port, proj_color, short_port};

/// 过滤后的容器列表（状态 / Compose 项目筛选 + 搜索）。
///
/// 行构建与概览带共用同一份过滤结果 —— 概览带的数字必须与列表一致。
pub fn filtered_containers(s: &AppState) -> Vec<&ContainerInfo> {
    s.containers
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
        .collect()
}

/// 按当前排序字段排序（升/降序由 `desc` 决定）
///
/// ⚠️ 所有分支的基线比较器都必须是**自然升序**（小的在前），`desc` 只负责反向。
///    曾经 CPU / 内存 / 磁盘三列的基线写成了降序，导致 `desc=true`（= UI 上的 ↓ 箭头）
///    反而从小到大排，与「名称」列和箭头方向全都不一致 —— 台账把排序提到表头后，
///    这个矛盾会直接暴露在主交互路径上，故在此统一口径。
pub fn sort_containers(
    mut list: Vec<&ContainerInfo>,
    field: SortField,
    desc: bool,
) -> Vec<&ContainerInfo> {
    match field {
        SortField::Group => {
            list.sort_by(|a, b| {
                let ap = a.compose_project.as_deref().unwrap_or("~");
                let bp = b.compose_project.as_deref().unwrap_or("~");
                let cmp = ap.cmp(bp).then_with(|| {
                    a.compose_service
                        .as_deref()
                        .cmp(&b.compose_service.as_deref())
                });
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Name => list.sort_by(|a, b| {
            let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Status => list.sort_by(|a, b| {
            // running 在前，按 name 次级
            let cmp = (b.is_running() as u8)
                .cmp(&(a.is_running() as u8))
                .then_with(|| a.name.cmp(&b.name));
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Cpu => list.sort_by(|a, b| {
            let cmp = a
                .cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal);
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Mem => list.sort_by(|a, b| {
            let cmp = a.mem_usage.cmp(&b.mem_usage);
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
        SortField::Disk => list.sort_by(|a, b| {
            let cmp = (a.block_read + a.block_write).cmp(&(b.block_read + b.block_write));
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        }),
    }
    list
}

/// 端口短标签（列内展示用）："8081 8083"
///
/// 只留宿主机端口号，完整映射（"8081→8081/tcp"）在抽屉详情里看。
fn port_short_labels(c: &ContainerInfo) -> String {
    if c.ports.trim().is_empty() {
        return String::new();
    }
    c.ports
        .split(", ")
        .filter(|p| !p.is_empty())
        .map(compact_port)
        .map(|p| short_port(&p).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 单个容器的台账行数据
///
/// `max_cpu` / `max_mem` 是**当前视图内**的最大值：微条长度按它归一，
/// 这样同一列里 12.4% 与 87.4% 直接可比（而不是各自对 100% 的占比）。
pub fn container_row(
    c: &ContainerInfo,
    max_cpu: f64,
    max_mem: u64,
    selected: bool,
) -> ContainerRow {
    let running = c.is_running();
    let cpu_ratio = if running && max_cpu > 0.0 {
        (c.cpu_percent / max_cpu).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mem_ratio = if running && max_mem > 0 {
        (c.mem_usage as f64 / max_mem as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ContainerRow {
        id: SharedString::from(c.id.clone()),
        name: SharedString::from(c.name.clone()),
        image: SharedString::from(c.image.clone()),
        is_running: running,
        is_selected: selected,
        needs_attention: c.needs_attention(),
        // 非 compose 容器置空（项目列只对真 compose 项目显示）
        compose_project: SharedString::from(c.compose_project.clone().unwrap_or_default()),
        compose_color: proj_color(c.compose_project.as_deref().unwrap_or_default()),
        // 台账要能一眼分出 12.4% 与 87.4% → 保留一位小数
        cpu_text: if running {
            SharedString::from(format!("{:.1}%", c.cpu_percent))
        } else {
            SharedString::from("—")
        },
        mem_text: if running {
            SharedString::from(format_bytes_compact(c.mem_usage))
        } else {
            SharedString::from("—")
        },
        disk: SharedString::from(format_bytes_compact(c.size_rw)),
        cpu_ratio: cpu_ratio as f32,
        mem_ratio: mem_ratio as f32,
        ports_text: SharedString::from(port_short_labels(c)),
    }
}
