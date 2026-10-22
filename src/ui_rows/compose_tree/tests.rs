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

/// 带内存配额（limit）的容器 —— 内存条按 usage/limit 算，必须有 limit
fn rcl(project: &str, service: &str, cpu: f64, mem: u64, mem_limit: u64) -> ContainerInfo {
    ContainerInfo {
        compose_project: (!project.is_empty()).then(|| project.to_string()),
        compose_service: (!service.is_empty()).then(|| service.to_string()),
        state: "running".to_string(),
        cpu_percent: cpu,
        mem_usage: mem,
        mem_limit,
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

/// 微条口径 = **docker stats 口径**（与 `docker stats` 的两个百分比同义）：
///   - CPU：相对**单核**的百分比 → 1.0 = 占满一核
///   - 内存：占 **limit** 的比例 → 1.0 = 占满配额
///
/// 分组带与容器行**同量纲**（都是「占上限的比例」），且
/// **组条 = 组内成员条之和** → 父子关系自洽，成员条不会被压掉分辨率。
#[test]
fn bars_use_docker_scale() {
    let state = state_with(vec![
        rcl("proj", "web", 50.0, 200, 1000),
        rcl("proj", "api", 20.0, 500, 1000),
    ]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);

    // 容器行：CPU 50% → 0.5；内存 200/1000 → 0.2
    let web = tree
        .rows
        .iter()
        .find(|r| r.kind == KIND_CONTAINER && r.cpu_text == "50.0%")
        .unwrap();
    assert!((web.cpu_ratio - 0.5).abs() < 0.001, "cpu {}", web.cpu_ratio);
    assert!((web.mem_ratio - 0.2).abs() < 0.001, "mem {}", web.mem_ratio);

    // 分组带 = 成员条之和（0.5 + 0.2 = 0.7；内存 0.2 + 0.5 = 0.7）
    let band = &tree.rows[0];
    assert_eq!(band.kind, KIND_PROJECT);
    assert!(
        (band.cpu_ratio - 0.7).abs() < 0.001,
        "band cpu {}",
        band.cpu_ratio
    );
    assert!(
        (band.mem_ratio - 0.7).abs() < 0.001,
        "band mem {}",
        band.mem_ratio
    );
}

/// 极小占比给 1px 可见下限，且**补偿在成员层做** → 组条 = 成员条之和仍成立
#[test]
fn tiny_ratios_keep_a_visible_floor_and_stay_additive() {
    let state = state_with(vec![
        rcl("proj", "a", 0.5, 1, 1000),
        rcl("proj", "b", 0.4, 1, 1000),
    ]);
    let st = state.lock().unwrap();
    let tree = build_ledger_tree(&st);

    let row = tree
        .rows
        .iter()
        .find(|r| r.kind == KIND_CONTAINER && r.cpu_text == "0.5%")
        .unwrap();
    // 0.5% → 0.005 → 抬到可见下限 MIN_BAR_RATIO
    let floor = super::super::containers::MIN_BAR_RATIO as f32;
    assert!((row.cpu_ratio - floor).abs() < 0.0001);

    // 组条 = 两个成员条之和（都是下限值）
    let band = &tree.rows[0];
    let expect = (super::super::containers::MIN_BAR_RATIO * 2.0) as f32;
    assert!(
        (band.cpu_ratio - expect).abs() < 0.0001,
        "band cpu {} vs {expect}",
        band.cpu_ratio
    );
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
