//! Compose 侧栏树状态：筛选范围 + 展开集合
//!
//! compose 选择与状态筛选（all / running / stopped）**正交**：
//! 台账过滤 = 状态筛选 ∧ compose 范围 ∧ 搜索。
//!
//! 纪律（AGENTS.md 规范 #18）：范围与展开集合都是**用户操作过的状态**——
//! 只由 UI 回调（`callbacks_compose.rs`）在 UI 线程写入；consumer 只读
//! （3s 刷新时按当前范围重建树的计数），绝不回写。

use std::collections::BTreeSet;

use crate::models::ContainerInfo;

/// compose 筛选范围
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ComposeScope {
    /// 不限 —— 全部容器
    #[default]
    All,
    /// 限定某个 compose 项目
    Project(String),
    /// 限定某个 compose 项目下的某个服务
    Service { project: String, service: String },
    /// 只看非 compose 容器
    Standalone,
}

impl ComposeScope {
    /// 该容器是否落在当前范围内
    pub fn matches(&self, c: &ContainerInfo) -> bool {
        match self {
            ComposeScope::All => true,
            ComposeScope::Project(p) => c.compose_project.as_deref() == Some(p.as_str()),
            ComposeScope::Service { project, service } => {
                c.compose_project.as_deref() == Some(project.as_str())
                    && c.compose_service.as_deref() == Some(service.as_str())
            }
            ComposeScope::Standalone => c.compose_project.as_deref().unwrap_or("").is_empty(),
        }
    }

    /// 该项目节点是否为当前选中项
    pub fn is_project_selected(&self, project: &str) -> bool {
        matches!(self, ComposeScope::Project(p) if p == project)
    }

    /// 该服务节点是否为当前选中项
    pub fn is_service_selected(&self, project: &str, service: &str) -> bool {
        matches!(self, ComposeScope::Service { project: p, service: sv } if p == project && sv == service)
    }

    /// 该项目子树里是否藏着选中项（选中的服务属于该项目）
    pub fn has_selection_in(&self, project: &str) -> bool {
        self.is_project_selected(project)
            || matches!(self, ComposeScope::Service { project: p, .. } if p == project)
    }

    /// 持久化编码：(kind, project, service)
    ///
    /// kind: 0=All 1=Project 2=Service 3=Standalone
    pub fn to_code(&self) -> (i64, String, String) {
        match self {
            ComposeScope::All => (0, String::new(), String::new()),
            ComposeScope::Project(p) => (1, p.clone(), String::new()),
            ComposeScope::Service { project, service } => (2, project.clone(), service.clone()),
            ComposeScope::Standalone => (3, String::new(), String::new()),
        }
    }

    /// 从持久化编码还原（越界 kind 回落 All）
    pub fn from_code(kind: i64, project: &str, service: &str) -> Self {
        match kind {
            1 if !project.is_empty() => ComposeScope::Project(project.to_string()),
            2 if !project.is_empty() && !service.is_empty() => ComposeScope::Service {
                project: project.to_string(),
                service: service.to_string(),
            },
            3 => ComposeScope::Standalone,
            _ => ComposeScope::All,
        }
    }
}

/// 侧栏树状态：当前范围 + 展开的项目集合
#[derive(Clone, Debug, Default)]
pub struct ComposeState {
    /// 当前 compose 范围
    pub scope: ComposeScope,
    /// 展开的项目名
    pub expanded: BTreeSet<String>,
}

impl ComposeState {
    /// 展开 / 收起某个项目，返回切换后的状态
    pub fn toggle(&mut self, project: &str) -> bool {
        if self.expanded.contains(project) {
            self.expanded.remove(project);
            false
        } else {
            self.expanded.insert(project.to_string());
            true
        }
    }

    /// 该项目是否展开
    pub fn is_expanded(&self, project: &str) -> bool {
        self.expanded.contains(project)
    }
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

    #[test]
    fn scope_matches_by_kind() {
        let plain = c("", "");
        let web = c("proj", "web");
        assert!(ComposeScope::All.matches(&plain));
        assert!(ComposeScope::Project("proj".into()).matches(&web));
        assert!(!ComposeScope::Project("proj".into()).matches(&plain));
        assert!(ComposeScope::Service {
            project: "proj".into(),
            service: "web".into()
        }
        .matches(&web));
        assert!(!ComposeScope::Service {
            project: "proj".into(),
            service: "db".into()
        }
        .matches(&web));
        assert!(ComposeScope::Standalone.matches(&plain));
        assert!(!ComposeScope::Standalone.matches(&web));
    }

    #[test]
    fn scope_code_roundtrip() {
        for scope in [
            ComposeScope::All,
            ComposeScope::Project("proj".into()),
            ComposeScope::Service {
                project: "proj".into(),
                service: "web".into(),
            },
            ComposeScope::Standalone,
        ] {
            let (k, p, s) = scope.to_code();
            assert_eq!(ComposeScope::from_code(k, &p, &s), scope);
        }
        // 越界 kind 回落 All
        assert_eq!(ComposeScope::from_code(9, "x", "y"), ComposeScope::All);
    }

    #[test]
    fn toggle_flips_expansion() {
        let mut st = ComposeState::default();
        assert!(st.toggle("proj"));
        assert!(st.is_expanded("proj"));
        assert!(!st.toggle("proj"));
        assert!(!st.is_expanded("proj"));
    }
}
