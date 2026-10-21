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

    // —— 4) 可见集合（= 渲染成行的容器）→ 归一化上限 / 概览带 / chips 计数 ——
    let visible: Vec<&ContainerInfo> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { c, .. } => Some(*c),
            Node::Band { .. } => None,
        })
        .collect();
    let max_cpu = visible.iter().fold(0.0f64, |m, c| m.max(c.cpu_percent));
    let max_mem = visible.iter().map(|c| c.mem_usage).max().unwrap_or(0);
    let summary = build_ledger_summary(&visible, max_cpu);

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
            } => rows.push(ContainerRow {
                id: SharedString::from(key.clone()),
                name: name.clone(),
                kind: *kind,
                depth: *depth,
                expanded: *expanded,
                group_n: *n as i32,
                compose_color: *color,
                ..Default::default()
            }),
            Node::Container { c, depth } => {
                let selected = s.batch.contains(&c.id);
                rows.push(container_row(c, max_cpu, max_mem, selected, *depth));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn c(project: &str, service: &str) -> ContainerInfo {
        ContainerInfo {
            compose_project: (!project.is_empty()).then(|| project.to_string()),
            compose_service: (!service.is_empty()).then(|| service.to_string()),
            ..Default::default()
        }
    }

    fn state_with(containers: Vec<ContainerInfo>) -> crate::ui_state::SharedState {
        let (state, _rx) = crate::ui_state::AppState::new();
        {
            let mut st = state.lock().unwrap();
            st.containers = containers;
        }
        state
    }

    #[test]
    fn project_band_with_single_container_service_has_no_service_band() {
        let state = state_with(vec![c("proj", "web"), c("proj", "db")]);
        let st = state.lock().unwrap();
        let tree = build_ledger_tree(&st);
        // 项目带 + 2 个容器行（单容器服务不出服务带）
        assert_eq!(tree.rows.len(), 3);
        assert_eq!(tree.rows[0].kind, KIND_PROJECT);
        assert_eq!(tree.rows[0].group_n, 2);
        assert!(tree.rows[0].expanded);
        assert_eq!(tree.rows[1].kind, KIND_CONTAINER);
        assert_eq!(tree.rows[1].depth, 1);
        assert_eq!(tree.visible.0, 2);
    }

    #[test]
    fn multi_container_service_emits_service_band() {
        let state = state_with(vec![c("proj", "web"), c("proj", "web"), c("proj", "db")]);
        let st = state.lock().unwrap();
        let tree = build_ledger_tree(&st);
        // 项目带 + 服务带 + 2 个容器行 + 单容器服务的 1 行
        assert_eq!(tree.rows.len(), 5);
        assert_eq!(tree.rows[0].kind, KIND_PROJECT);
        // 服务带按服务名排序（db < web），db 只有 1 个容器 → 不出服务带
        let bands: Vec<&ContainerRow> = tree
            .rows
            .iter()
            .filter(|r| r.kind == KIND_SERVICE)
            .collect();
        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].name, "web");
        assert_eq!(bands[0].depth, 1);
        assert_eq!(bands[0].group_n, 2);
        // 服务带下的容器行缩进 2 级
        let deep: Vec<&ContainerRow> = tree
            .rows
            .iter()
            .filter(|r| r.kind == KIND_CONTAINER && r.depth == 2)
            .collect();
        assert_eq!(deep.len(), 2);
    }

    #[test]
    fn collapsed_group_hides_its_rows_and_shrinks_counts() {
        let state = state_with(vec![c("proj", "web"), c("proj", "web"), c("", "")]);
        let mut st = state.lock().unwrap();
        st.compose_collapsed.insert(project_key("proj"));
        let tree = build_ledger_tree(&st);
        // 项目带（收起）+ 顶层容器行（无归属，不受项目收起影响）
        assert_eq!(tree.rows.len(), 2);
        assert!(!tree.rows[0].expanded);
        assert_eq!(tree.rows[1].kind, KIND_CONTAINER);
        assert_eq!(tree.rows[1].depth, 0);
        assert_eq!(tree.visible.0, 1);
    }

    #[test]
    fn standalone_containers_stay_at_top_level_without_band() {
        let state = state_with(vec![c("", ""), c("proj", "web")]);
        let st = state.lock().unwrap();
        let tree = build_ledger_tree(&st);
        let kinds: Vec<i32> = tree.rows.iter().map(|r| r.kind).collect();
        // 项目带 → 项目下的容器行 → 顶层容器行（无分组带）
        assert_eq!(kinds, vec![KIND_PROJECT, KIND_CONTAINER, KIND_CONTAINER]);
        assert_eq!(tree.rows[2].depth, 0);
        assert_eq!(tree.visible.0, 2);
    }
}
