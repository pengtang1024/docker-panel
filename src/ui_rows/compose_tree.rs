//! Compose 侧栏树节点构建（扁平可见列表）
//!
//! 树结构：全部容器 / 项目（展开 → 服务）/ 独立容器。
//!
//! 输出是**扁平**列表：展开的项目后面紧跟它的服务节点，层级由 `depth`
//! 表达（Slint 侧按 depth 缩进）。收起的服务不产出节点。
//!
//! 计数口径：项目 / 服务节点的数字是该组容器的**绝对数量**（不随状态筛选
//! 「运行中 / 已停止」变化）——与 Docker Desktop 侧栏一致。

use std::collections::BTreeMap;

use crate::compose_scope::ComposeScope;
use crate::i18n::t;
use slint::SharedString;

use crate::slint_gen::ComposeNode;
use crate::ui_state::AppState;

use super::proj_color;

/// 节点 kind（与 Slint 侧 `ComposeNode.kind` 一致）
pub const KIND_ALL: i32 = 0;
pub const KIND_PROJECT: i32 = 1;
pub const KIND_SERVICE: i32 = 2;
pub const KIND_STANDALONE: i32 = 3;

/// 构建侧栏树的可见节点列表
pub fn build_compose_tree(s: &AppState) -> Vec<ComposeNode> {
    // 项目 → 服务 → 容器数
    let mut projects: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut standalone = 0usize;
    for c in &s.containers {
        match c.compose_project.as_deref().filter(|p| !p.is_empty()) {
            Some(p) => {
                let svc = c.compose_service.as_deref().unwrap_or("");
                *projects.entry(p).or_default().entry(svc).or_default() += 1;
            }
            None => standalone += 1,
        }
    }

    let mut nodes: Vec<ComposeNode> = Vec::with_capacity(projects.len() * 2 + 2);

    // —— 顶层：全部容器 ——
    nodes.push(ComposeNode {
        kind: KIND_ALL,
        depth: 0,
        project: SharedString::new(),
        service: SharedString::new(),
        label: SharedString::from(t("All containers")),
        color: proj_color(""),
        n: s.containers.len() as i32,
        expanded: false,
        selected: matches!(s.compose.scope, ComposeScope::All),
        contains_selected: false,
    });

    // —— 项目（+ 展开时的服务）——
    for (project, services) in &projects {
        let project_n: usize = services.values().sum();
        let expanded = s.compose.is_expanded(project);
        nodes.push(ComposeNode {
            kind: KIND_PROJECT,
            depth: 0,
            project: SharedString::from(*project),
            service: SharedString::new(),
            label: SharedString::from(*project),
            color: proj_color(project),
            n: project_n as i32,
            expanded,
            selected: s.compose.scope.is_project_selected(project),
            // 选中的服务藏在本项目里 → 收起时也留个记号，别让筛选凭空消失
            contains_selected: s.compose.scope.has_selection_in(project),
        });
        if !expanded {
            continue;
        }
        for (service, n) in services {
            nodes.push(ComposeNode {
                kind: KIND_SERVICE,
                depth: 1,
                project: SharedString::from(*project),
                service: SharedString::from(*service),
                label: SharedString::from(if service.is_empty() {
                    t("Unnamed service")
                } else {
                    (*service).to_string()
                }),
                color: proj_color(project),
                n: *n as i32,
                expanded: false,
                selected: s.compose.scope.is_service_selected(project, service),
                contains_selected: false,
            });
        }
    }

    // —— 顶层：独立容器（非 compose）——
    if standalone > 0 {
        nodes.push(ComposeNode {
            kind: KIND_STANDALONE,
            depth: 0,
            project: SharedString::new(),
            service: SharedString::new(),
            label: SharedString::from(t("Standalone containers")),
            color: proj_color(""),
            n: standalone as i32,
            expanded: false,
            selected: matches!(s.compose.scope, ComposeScope::Standalone),
            contains_selected: false,
        });
    }

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContainerInfo;

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
    fn tree_lists_all_project_and_standalone() {
        let state = state_with(vec![
            c("proj-a", "web"),
            c("proj-a", "web"),
            c("proj-a", "db"),
            c("", ""),
        ]);
        let mut st = state.lock().unwrap();
        // ⚠️ AppState::new() 会读用户配置文件 → 范围可能是上次留下的，
        // 测试必须显式设定，不能依赖默认值
        st.compose.scope = ComposeScope::All;
        let nodes = build_compose_tree(&st);
        // 全部容器 + proj-a + 独立容器（项目未展开 → 无服务节点）
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].kind, KIND_ALL);
        assert_eq!(nodes[0].n, 4);
        assert!(nodes[0].selected);
        assert_eq!(nodes[1].kind, KIND_PROJECT);
        assert_eq!(nodes[1].label, "proj-a");
        assert_eq!(nodes[1].n, 3);
        assert_eq!(nodes[2].kind, KIND_STANDALONE);
        assert_eq!(nodes[2].n, 1);
    }

    #[test]
    fn expanded_project_emits_service_nodes() {
        let state = state_with(vec![c("proj-a", "web"), c("proj-a", "db"), c("", "")]);
        let mut st = state.lock().unwrap();
        st.compose.scope = ComposeScope::All;
        st.compose.expanded.insert("proj-a".to_string());
        let nodes = build_compose_tree(&st);
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[1].kind, KIND_PROJECT);
        assert!(nodes[1].expanded);
        // 服务按名字排序：db 在 web 前
        assert_eq!(nodes[2].kind, KIND_SERVICE);
        assert_eq!(nodes[2].label, "db");
        assert_eq!(nodes[2].depth, 1);
        assert_eq!(nodes[3].label, "web");
        assert_eq!(nodes[4].kind, KIND_STANDALONE);
    }

    #[test]
    fn collapsed_project_keeps_selection_marker() {
        let state = state_with(vec![c("proj-a", "web")]);
        let mut st = state.lock().unwrap();
        // 选中的服务藏在收起的项目里 → 项目节点要留记号，否则筛选凭空消失
        st.compose.scope = ComposeScope::Service {
            project: "proj-a".into(),
            service: "web".into(),
        };
        let nodes = build_compose_tree(&st);
        assert_eq!(nodes[1].kind, KIND_PROJECT);
        assert!(!nodes[1].expanded);
        assert!(!nodes[1].selected);
        assert!(nodes[1].contains_selected);
    }

    #[test]
    fn selected_scope_marks_node() {
        let state = state_with(vec![c("proj-a", "web")]);
        let mut st = state.lock().unwrap();
        st.compose.scope = ComposeScope::Project("proj-a".into());
        let nodes = build_compose_tree(&st);
        assert!(!nodes[0].selected);
        assert!(nodes[1].selected);
    }
}
