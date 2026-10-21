//! 台账内联树行构建单测
//!
//! 覆盖：项目带 / 服务带的行型与层级、展开收起与计数联动、
//! 以及分组带的资源汇总（成员之和）口径。

use super::*;

fn c(project: &str, service: &str) -> ContainerInfo {
    ContainerInfo {
        compose_project: (!project.is_empty()).then(|| project.to_string()),
        compose_service: (!service.is_empty()).then(|| service.to_string()),
        ..Default::default()
    }
}

/// 带资源读数的容器（running）
fn rc(project: &str, service: &str, cpu: f64, mem: u64, disk: u64) -> ContainerInfo {
    ContainerInfo {
        compose_project: (!project.is_empty()).then(|| project.to_string()),
        compose_service: (!service.is_empty()).then(|| service.to_string()),
        state: "running".to_string(),
        cpu_percent: cpu,
        mem_usage: mem,
        size_rw: disk,
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

#[test]
fn band_totals_are_sum_of_members() {
    let state = state_with(vec![
        rc("proj", "web", 10.0, 100, 1000),
        rc("proj", "web", 5.5, 200, 2000),
    ]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);
    let band = &tree.rows[0];
    assert_eq!(band.kind, KIND_PROJECT);
    // 项目带汇总 = 全部成员之和
    assert_eq!(band.cpu_text, "15.5%");
    assert_eq!(band.mem_text, "300B");
    assert_eq!(band.disk, "2K");
    // 服务带汇总 = 该服务自己的成员之和（与项目带同源同值）
    let svc = tree.rows.iter().find(|r| r.kind == KIND_SERVICE).unwrap();
    assert_eq!(svc.cpu_text, "15.5%");
    assert_eq!(svc.mem_text, "300B");
}

#[test]
fn collapsed_band_keeps_totals() {
    let state = state_with(vec![rc("proj", "web", 12.0, 500, 1000)]);
    let mut st = state.lock().unwrap();
    st.compose_collapsed.insert(project_key("proj"));
    let tree = build_ledger_tree(&st);
    // 收起 → 行不渲染，但汇总描述的是「这个组占了多少」→ 数字照旧
    assert_eq!(tree.rows.len(), 1);
    assert!(!tree.rows[0].expanded);
    assert_eq!(tree.rows[0].cpu_text, "12.0%");
    assert_eq!(tree.rows[0].mem_text, "500B");
    assert_eq!(tree.rows[0].disk, "1000B");
}

#[test]
fn stopped_members_count_for_disk_only() {
    let state = state_with(vec![
        rc("proj", "web", 10.0, 100, 1000),
        ContainerInfo {
            compose_project: Some("proj".to_string()),
            compose_service: Some("web".to_string()),
            state: "exited".to_string(),
            cpu_percent: 20.0,
            mem_usage: 500,
            size_rw: 2000,
            ..Default::default()
        },
    ]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);
    let band = &tree.rows[0];
    // 停止的容器没有实时占用 → CPU / 内存不计；磁盘是累计值 → 照计
    assert_eq!(band.cpu_text, "10.0%");
    assert_eq!(band.mem_text, "100B");
    assert_eq!(band.disk, "2K");
}

#[test]
fn band_and_rows_share_one_bar_baseline() {
    // A group with 2 members: the group's total (20.0%) > the largest single container (10.0%)
    // → the whole column shares one baseline (20.0%): the group is full, the container is half
    let state = state_with(vec![
        rc("proj", "web", 10.0, 300, 1000),
        rc("proj", "web", 10.0, 100, 500),
    ]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);
    let band = tree.rows.iter().find(|r| r.kind == KIND_PROJECT).unwrap();
    let row = tree.rows.iter().find(|r| r.kind == KIND_CONTAINER).unwrap();
    // The group's total is the column's max → full bar
    assert_eq!(band.cpu_text, "20.0%");
    assert_eq!(band.cpu_ratio, 1.0);
    // The row's bar is scaled by the same baseline → half
    assert!((row.cpu_ratio - 0.5).abs() < 0.01);
    // Memory: the group's total (400M) is the column's max → full bar; the row (300M) → 3/4
    assert_eq!(band.mem_ratio, 1.0);
    assert!((row.mem_ratio - 0.75).abs() < 0.01);
}

#[test]
fn band_without_running_members_shows_dash_for_cpu_and_mem() {
    let state = state_with(vec![c("proj", "web")]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);
    let band = &tree.rows[0];
    assert_eq!(band.cpu_text, "—");
    assert_eq!(band.mem_text, "—");
}
