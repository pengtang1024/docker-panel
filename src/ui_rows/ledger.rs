//! 台账概览带 + 台账入口（方向 A）
//!
//! 概览带 = 总数 / 运行中 / 已停止 / 需关注 + CPU 占用 TOP4 条形。
//! 数字口径 = **当前筛选后的列表**（与台账行一致），与筛选栏芯片的
//! 全量计数不同 —— 选中「运行中」时这里显示的是筛选后的数。

use slint::SharedString;

use crate::models::ContainerInfo;
use crate::slint_gen::{ContainerRow, LedgerSummary};
use crate::ui_state::AppState;

use super::containers::{container_row, filtered_containers, sort_containers};

/// 概览带：4 个计数 + CPU TOP4（名字 / 百分比 / 归一化比例）
///
/// `max_cpu` 由调用侧算好（当前视图内最大值）传入 —— 概览带的条长
/// 与列表行的条长用同一个归一化基准，两处视觉才一致。
pub fn build_ledger_summary(list: &[&ContainerInfo], max_cpu: f64) -> LedgerSummary {
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
        let ratio = if max_cpu > 0.0 {
            (c.cpu_percent / max_cpu).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        ratios.push(ratio);
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

/// 构建台账（行 + 概览带）
///
/// 行与概览带出自**同一份过滤结果**：概览带说「38 个运行中」时，
/// 列表里就该是 38 行。两处分批算就会在筛选边界上自相矛盾。
pub fn build_container_ledger(s: &AppState) -> (Vec<ContainerRow>, LedgerSummary) {
    let list = filtered_containers(s);
    // 归一化上限取**当前视图内**的最大值（列内可比，不是绝对占比）
    let max_cpu = list.iter().fold(0.0f64, |m, c| m.max(c.cpu_percent));
    let max_mem = list.iter().map(|c| c.mem_usage).max().unwrap_or(0);
    let summary = build_ledger_summary(&list, max_cpu);

    let rows = sort_containers(list, s.sort_field, s.sort_desc)
        .into_iter()
        .map(|c| {
            let selected = s.batch.contains(&c.id);
            container_row(c, max_cpu, max_mem, selected)
        })
        .collect();
    (rows, summary)
}
