//! 台账内联树：项目带 / 服务带 + 容器行的**混合列表**
//!
//! 树深 1-2 级：
//!
//! ```text
//! ▾ ● live-games (5)          ← 项目带（kind=1，通栏分节）
//!     live-service-1          ← 容器行（depth=1）
//!     ▾ web (3)               ← 服务带（kind=2，仅多容器服务才有）
//!         web-1                ← 容器行（depth=2）
//! ```
//!
//! 服务带**只在该服务有 ≥2 个容器时出现**——一服务一容器的常见 compose 项目下
//! 树只有 1 级，不会给每个容器套一层空壳。
//!
//! 展开 / 收起状态存在 `AppState.compose_collapsed`（存的是**收起**的分组键，
//! 不在集合里 = 展开）→ 新出现的分组默认展开。
//!
//! 计数口径：概览带与筛选栏 chips 一律取**可见容器集合**（= 实际渲染成行的那些
//! 容器），保持「概览带数字 = 列表行数」这条既有不变量。

use std::collections::BTreeMap;

use slint::SharedString;

use crate::models::ContainerInfo;
use crate::slint_gen::{ContainerRow, LedgerSummary};
use crate::ui_state::AppState;

use super::containers::{container_row, filtered_containers, sort_containers};
use super::group_totals::{self, GroupTotals};
use super::ledger::build_ledger_summary;
use super::proj_color;

/// 行型：容器行
pub const KIND_CONTAINER: i32 = 0;
/// 行型：项目带
pub const KIND_PROJECT: i32 = 1;
/// 行型：服务带
pub const KIND_SERVICE: i32 = 2;

/// 项目分组键（空项目名 = 「独立容器」伪分组）
pub fn project_key(project: &str) -> String {
    format!("project:{project}")
}

/// 服务分组键
pub fn service_key(project: &str, service: &str) -> String {
    format!("svc:{project}/{service}")
}

/// 内联树的一个节点（分节带 或 容器行）
enum Node<'a> {
    /// 分节带：项目 / 服务
    Band {
        kind: i32,
        depth: i32,
        key: String,
        name: SharedString,
        color: slint::Color,
        n: usize,
        expanded: bool,
        /// 资源汇总（组内成员之和）
        totals: GroupTotals,
    },
    /// 容器行
    Container { c: &'a ContainerInfo, depth: i32 },
}

/// 内联树构建结果
pub struct TreeLedger {
    /// 混合行列表（分节带 + 容器行，按渲染顺序）
    pub rows: Vec<ContainerRow>,
    /// 概览带（口径 = 可见容器集合）
    pub summary: LedgerSummary,
    /// 可见容器数 / 其中运行中（筛选栏 chips 口径）
    pub visible: (usize, usize),
}

/// 构建台账内联树
pub fn build_ledger_tree(s: &AppState) -> TreeLedger {
    let filtered = filtered_containers(s);

    // —— 1) 项目分组（无 compose 标签的进「独立容器」）——
    let mut by_project: BTreeMap<&str, Vec<&ContainerInfo>> = BTreeMap::new();
    let mut standalone: Vec<&ContainerInfo> = Vec::new();
    for c in filtered.iter().copied() {
        match c.compose_project.as_deref().filter(|p| !p.is_empty()) {
            Some(p) => by_project.entry(p).or_default().push(c),
            None => standalone.push(c),
        }
    }

    // —— 2) 组装节点（分节带 + 容器行）——
    // 收起 / 展开状态存的是「收起」的分组键；不在集合里 = 展开
    let collapsed = &s.compose_collapsed;
    let mut nodes: Vec<Node> = Vec::new();

    for (project, members) in &by_project {
        let pkey = project_key(project);
        let expanded = !collapsed.contains(&pkey);
        nodes.push(Node::Band {
            kind: KIND_PROJECT,
            depth: 0,
            key: pkey,
            name: SharedString::from(*project),
            color: proj_color(project),
            n: members.len(),
            expanded,
            // 项目带汇总 = 该项目**全部**容器之和（含服务带下的容器）
            totals: GroupTotals::of(members),
        });
        if !expanded {
            continue;
        }
        // 服务再分组（服务名为空 = 没带 service 标签）
        let mut by_service: BTreeMap<&str, Vec<&ContainerInfo>> = BTreeMap::new();
        for c in members.iter().copied() {
            by_service
                .entry(c.compose_service.as_deref().unwrap_or(""))
                .or_default()
                .push(c);
        }
        for (service, svc_members) in &by_service {
            // 多容器服务 → 插入服务带（第 2 级）；单容器服务直接挂容器行
            if svc_members.len() >= 2 && !service.is_empty() {
                let skey = service_key(project, service);
                let sexpanded = !collapsed.contains(&skey);
                nodes.push(Node::Band {
                    kind: KIND_SERVICE,
                    depth: 1,
                    key: skey,
                    name: SharedString::from(*service),
                    color: proj_color(project),
                    n: svc_members.len(),
                    expanded: sexpanded,
                    // 服务带汇总 = 该服务自己的容器之和
                    totals: GroupTotals::of(svc_members),
                });
                if !sexpanded {
                    continue;
                }
                for c in sort_containers(svc_members.clone(), s.sort_field, s.sort_desc) {
                    nodes.push(Node::Container { c, depth: 2 });
                }
            } else {
                for c in sort_containers(svc_members.clone(), s.sort_field, s.sort_desc) {
                    nodes.push(Node::Container { c, depth: 1 });
                }
            }
        }
    }

    // —— 3) 顶层容器：没有 compose 项目归属的容器**不分组**，直接排在列表顶层 ——
    //    （「独立容器」不带分组带：树只表达 compose 层级，无归属就是顶层）
    for c in sort_containers(standalone, s.sort_field, s.sort_desc) {
        nodes.push(Node::Container { c, depth: 0 });
    }

    // —— 4) 可见集合（= 渲染成行的容器）→ 概览带 / chips 计数 ——
    let visible: Vec<&ContainerInfo> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { c, .. } => Some(*c),
            Node::Band { .. } => None,
        })
        .collect();
    let summary = build_ledger_summary(&visible);

    // —— 4b) 微条刻度：**docker 口径的绝对占比**（不再按视图内最大值归一）——
    //
    // CPU = 相对单核的百分比 / 100（1.0 = 占满一核）
    // 内存 = usage / limit（1.0 = 占满配额）
    //
    // 为什么不用「视图内最大值归一」：那种相对刻度下**分组带与容器行量纲不同**
    // （分组是成员之和，成员是个体）→ 同一列里出现「组数字是成员的 2 倍、
    // 条却一样长」（唯一分组时组条恒 100% 满格，零信息量）。
    // 绝对口径下两者同量纲（都是「占上限的比例」），且
    // **分组带 = 组内成员条之和** → 父子关系自洽，成员条不会被压掉分辨率。
    //
    // 代价：低负载机器上绝对占比很小，条偏短 —— 精确值由数字给出。
    // 概览带 TOP4 的条同样用这套基准（见 `build_ledger_summary`）。

    // —— 5) 出行 ——
    let mut rows: Vec<ContainerRow> = Vec::with_capacity(nodes.len());
    for node in &nodes {
        match node {
            Node::Band {
                kind,
                depth,
                key,
                name,
                color,
                n,
                expanded,
                totals,
            } => rows.push(ContainerRow {
                id: SharedString::from(key.clone()),
                name: name.clone(),
                kind: *kind,
                depth: *depth,
                expanded: *expanded,
                is_running: *n > 0 && totals.running() == *n,
                group_n: *n as i32,
                // 组内运行中成员数：< group_n 说明分组还没全部起来
                // （分组带主按钮据此在「启动全组 / 重启全组」之间切换）
                group_running: totals.running() as i32,
                compose_color: *color,
                // 资源汇总：复用容器行的三个字段（分组带渲染它们）
                cpu_text: totals.cpu_text(),
                mem_text: totals.mem_text(),
                disk: totals.disk_text(),
                // 微条：分组带 = 组内成员条之和（与容器行同量纲，可直接比较）
                cpu_ratio: group_totals::ratio_abs(totals.cpu_ratio()),
                mem_ratio: group_totals::ratio_abs(totals.mem_ratio()),
                ..Default::default()
            }),
            Node::Container { c, depth } => {
                let selected = s.batch.contains(&c.id);
                rows.push(container_row(c, selected, *depth));
            }
        }
    }

    let visible_total = visible.len();
    let visible_running = visible.iter().filter(|c| c.is_running()).count();
    TreeLedger {
        rows,
        summary,
        visible: (visible_total, visible_running),
    }
}

/// 分组键解析结果：(项目名, 可选服务名)
fn parse_group_key(key: &str) -> Option<(&str, Option<&str>)> {
    if let Some(project) = key.strip_prefix("project:") {
        return (!project.is_empty()).then_some((project, None));
    }
    let rest = key.strip_prefix("svc:")?;
    let (project, service) = rest.split_once('/')?;
    (!project.is_empty() && !service.is_empty()).then_some((project, Some(service)))
}

/// 分组键 → 展示名（抽屉标题）：`project:P` → P，`svc:P/S` → "P / S"
pub fn group_label(key: &str) -> String {
    match parse_group_key(key) {
        Some((project, Some(service))) => format!("{project} / {service}"),
        Some((project, None)) => project.to_string(),
        None => key.to_string(),
    }
}

/// 分组的一个成员（组操作的执行对象）
pub struct GroupMember {
    pub id: String,
    pub name: String,
    pub running: bool,
}

/// 分组键 → 组内成员（口径 = 当前**筛选后**的容器，与台账可见行一致）
///
/// 分组带只表达筛选结果里的成员：组操作也必须只动这批容器，
/// 否则「筛出 2 个容器点停止全组」会顺带停掉筛选外看不见的成员。
pub fn group_members(s: &AppState, key: &str) -> Vec<GroupMember> {
    let Some((project, service)) = parse_group_key(key) else {
        return Vec::new();
    };
    filtered_containers(s)
        .into_iter()
        .filter(|c| c.compose_project.as_deref() == Some(project))
        .filter(|c| match service {
            Some(svc) => c.compose_service.as_deref().unwrap_or("") == svc,
            None => true,
        })
        .map(|c| GroupMember {
            id: c.id.clone(),
            name: c.name.clone(),
            running: c.is_running(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
