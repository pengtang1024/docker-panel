//! 容器台账行构建（筛选 + 排序 + 资源占比比例 + 异常标记）
//!
//! 台账形态（方向 A）：一行一个容器，列起点固定、数值右对齐等宽体，
//! CPU / 内存用微条表示，**长度 = docker stats 口径的绝对占比**
//! （CPU = 相对单核的百分比 / 100，内存 = usage / limit）——
//! 与容器旁的数字同源，与分组带的条（成员条之和）同量纲。

use slint::SharedString;

use crate::detail_fmt::format_bytes_compact;
use crate::models::{ContainerInfo, SortField};
use crate::slint_gen::ContainerRow;
use crate::ui_state::AppState;

use super::{compact_port, proj_color, short_port};

/// 过滤后的容器列表（状态筛选 ∧ 搜索）。
///
/// 行构建与概览带共用同一份过滤结果 —— 概览带的数字必须与列表一致。
/// compose 层级**不参与过滤**（树是分组不是筛选器，见 ui_rows/compose_tree.rs）。
pub fn filtered_containers(s: &AppState) -> Vec<&ContainerInfo> {
    s.containers
        .iter()
        .filter(|c| {
            // 状态筛选（筛选栏 chips：all / running / stopped）
            let running = c.is_running();
            match s.filter.as_str() {
                "running" if !running => return false,
                "stopped" if running => return false,
                _ => {}
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
            // 排序键必须与台账「磁盘」列的显示值同源（size_rw = 可写层占用），
            // 否则点表头排序看到的顺序与列里的数字对不上。
            // 曾经用 block_read + block_write 排序，而这两个字段从未被赋值（恒 0）
            // → 所有容器排序键相等，稳定排序保持原序 → 点「磁盘」排序毫无反应。
            let cmp = a.size_rw.cmp(&b.size_rw);
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

/// 微条的最小可见比例（约等于 1px / 52px 轨道）
///
/// docker 口径是绝对占比，低负载机器上真实比例可能只有 0.01（0.5px）→
/// 几乎看不见。给**大于 0** 的比例加一个 1px 的可见下限，
/// 精确值由旁边的数字给出。
///
/// ⚠️ 补偿必须在**成员**这一层做，分组带再把补偿后的成员条相加
/// → 「组条 = 成员条之和」严格成立（否则组条会比成员条之和还短，父子反向失真）。
pub const MIN_BAR_RATIO: f64 = 0.02;

/// 微条比例：docker 口径 + 最小可见补偿（0 → 0，不画条）
pub fn bar_ratio(value: f64) -> f64 {
    if value > 0.0 {
        value.clamp(MIN_BAR_RATIO, 1.0)
    } else {
        0.0
    }
}

/// 内存微条比例（docker stats 的 MEM% 口径）：占 **limit** 的比例
///
/// limit 缺失（停止的容器没有 stats 采样）→ 0（行上本来就显示「—」）。
pub fn mem_ratio(c: &ContainerInfo) -> f64 {
    if c.mem_limit == 0 {
        0.0
    } else {
        c.mem_usage as f64 / c.mem_limit as f64
    }
}

/// 单个容器的台账行数据
///
/// **微条口径 = docker stats 口径**（与 `docker stats` 的两个百分比同义）：
///   - CPU：相对**单核**的百分比 → 1.0 = 占满一核
///   - 内存：占 **limit** 的比例 → 1.0 = 占满配额
///
/// 刻意不用「视图内最大值归一」：那种相对刻度让**分组带与容器行量纲不同**
/// （分组总和 vs 单容器），同一列里会出现「组数字是成员的 2 倍、条却一样长」。
/// 绝对口径下两者同量纲（都是占上限的比例），且分组带 = 成员条之和。
pub fn container_row(c: &ContainerInfo, selected: bool, depth: i32) -> ContainerRow {
    let running = c.is_running();
    // CPU：docker 口径已是「相对单核的百分比」→ /100 即条长比例
    let cpu_ratio = if running {
        bar_ratio(c.cpu_percent / 100.0)
    } else {
        0.0
    };
    let mem_ratio = if running {
        bar_ratio(mem_ratio(c))
    } else {
        0.0
    };
    ContainerRow {
        id: SharedString::from(c.id.clone()),
        name: SharedString::from(c.name.clone()),
        image: SharedString::from(c.image.clone()),
        // 内联树：容器行（kind=0），depth = 所属层级
        kind: super::compose_tree::KIND_CONTAINER,
        depth,
        expanded: false,
        group_n: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn with_disk(name: &str, size_rw: u64) -> ContainerInfo {
        ContainerInfo {
            name: name.to_string(),
            size_rw,
            ..Default::default()
        }
    }

    /// 「磁盘」列显示的是 size_rw → 排序键必须同源，
    /// 否则点表头排序看到的顺序与列里的数字对不上（曾经用恒 0 的 block_read/block_write）。
    #[test]
    fn disk_sort_uses_size_rw() {
        let list = [
            with_disk("mid", 500),
            with_disk("big", 900),
            with_disk("small", 100),
        ];
        let refs: Vec<&ContainerInfo> = list.iter().collect();

        let asc = sort_containers(refs.clone(), SortField::Disk, false);
        assert_eq!(
            asc.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["small", "mid", "big"]
        );

        let desc = sort_containers(refs, SortField::Disk, true);
        assert_eq!(
            desc.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["big", "mid", "small"]
        );
    }
}
