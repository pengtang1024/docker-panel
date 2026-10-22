//! 台账概览带 + 台账入口（方向 A）
//!
//! 概览带 = 总数 / 运行中 / 已停止 / 需关注 + CPU 占用 TOP4 条形。
//! 数字口径 = **当前可见的容器集合**（= 内联树实际渲染成行的那些容器），
//! 与台账行数一致 —— 收起一个分组时两边同步变小。

use slint::SharedString;

use crate::models::ContainerInfo;
use crate::slint_gen::{ContainerRow, LedgerSummary};
use crate::ui_state::AppState;

use super::compose_tree::build_ledger_tree;

/// 概览带：4 个计数 + CPU TOP4（名字 / 百分比 / 条长比例）
///
/// TOP4 的条用 **docker 口径**（与台账行、概览带同一基准）：
/// 比例 = 相对单核的百分比 / 100（1.0 = 占满一核）。
/// 这样概览带与台账里同一个容器的条**长度可直接比较**（轨道不同但比例相同）。
pub fn build_ledger_summary(list: &[&ContainerInfo]) -> LedgerSummary {
    let total = list.len() as i32;
    let running = list.iter().filter(|c| c.is_running()).count() as i32;
    let attention = list.iter().filter(|c| c.needs_attention()).count() as i32;

    // CPU TOP4：只取运行中且有实际占用的
    let mut top: Vec<&ContainerInfo> = list
        .iter()
        .copied()
        .filter(|c| c.is_running() && c.cpu_percent > 0.0)
        .collect();
    top.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    top.truncate(4);

    let mut names: Vec<SharedString> = Vec::with_capacity(4);
    let mut pcts: Vec<SharedString> = Vec::with_capacity(4);
    let mut ratios: Vec<f32> = Vec::with_capacity(4);
    for c in &top {
        names.push(SharedString::from(c.name.clone()));
        pcts.push(SharedString::from(format!("{:.1}%", c.cpu_percent)));
        // docker 口径：相对单核的百分比 → /100 即条长比例（1.0 = 占满一核）
        ratios.push((c.cpu_percent / 100.0).clamp(0.0, 1.0) as f32);
    }
    // 不足 4 个用空槽补齐（Slint struct 内禁止嵌套数组，否则失去 Send）
    while names.len() < 4 {
        names.push(SharedString::default());
        pcts.push(SharedString::default());
        ratios.push(0.0);
    }

    LedgerSummary {
        total,
        running,
        stopped: total - running,
        attention,
        top_name1: names[0].clone(),
        top_pct1: pcts[0].clone(),
        top_ratio1: ratios[0],
        top_name2: names[1].clone(),
        top_pct2: pcts[1].clone(),
        top_ratio2: ratios[1],
        top_name3: names[2].clone(),
        top_pct3: pcts[2].clone(),
        top_ratio3: ratios[2],
        top_name4: names[3].clone(),
        top_pct4: pcts[3].clone(),
        top_ratio4: ratios[3],
    }
}

/// 构建台账（内联树行 + 概览带 + 可见计数）
///
/// 行与概览带出自**同一份可见集合**：概览带说「38 个运行中」时，
/// 列表里就该是 38 个容器行。两处分批算就会在折叠 / 筛选边界上自相矛盾。
pub fn build_container_ledger(s: &AppState) -> (Vec<ContainerRow>, LedgerSummary, (usize, usize)) {
    let tree = build_ledger_tree(s);
    (tree.rows, tree.summary, tree.visible)
}
