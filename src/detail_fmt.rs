//! 详情/镜像详情字段格式化：AppState 数据 → SharedString 元组
//!
//! 独立模块避免 ui_state.rs 超过 300 行。

use slint::SharedString;

use crate::ui_state::{AppState, DetailTuple};

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// 锁内调用：把容器详情格式化为 18 个 SharedString
pub fn build_detail_data(s: &AppState) -> Option<DetailTuple> {
    let d = s.detail.as_ref()?;
    let fmt_list = |list: &[String]| -> String {
        if list.is_empty() {
            "—".into()
        } else {
            list.join(", ")
        }
    };
    let cpu = s
        .containers
        .iter()
        .find(|c| c.id == d.id)
        .map(|c| format!("{:.1}%", c.cpu_percent))
        .unwrap_or_default();
    let mem = s
        .containers
        .iter()
        .find(|c| c.id == d.id)
        .map(|c| {
            format!(
                "{} / {}",
                format_bytes(c.mem_usage),
                format_bytes(c.mem_limit)
            )
        })
        .unwrap_or_default();
    Some((
        SharedString::from(d.name.clone()),
        SharedString::from(d.state.clone()),
        SharedString::from(d.status.clone()),
        SharedString::from(d.image.clone()),
        SharedString::from(d.id.clone()),
        SharedString::from(d.pid.to_string()),
        SharedString::from("—"),
        SharedString::from(cpu),
        SharedString::from(mem),
        SharedString::from(d.restart_policy.clone()),
        SharedString::from(d.created.clone()),
        SharedString::from(d.started_at.clone()),
        SharedString::from(d.network_mode.clone()),
        SharedString::from(fmt_list(&d.ports)),
        SharedString::from(fmt_list(&d.mounts)),
        SharedString::from(fmt_list(&d.networks)),
        SharedString::from(d.entrypoint.clone()),
        SharedString::from(d.command.clone()),
    ))
}

/// 锁内调用：把镜像详情格式化（6 字段 SharedString 列表）
pub fn build_image_detail_data(s: &AppState) -> Option<Vec<SharedString>> {
    let d = s.image_detail.as_ref()?;
    Some(vec![
        SharedString::from(d.repo_tags.clone()),
        SharedString::from(d.size.clone()),
        SharedString::from(format!("{}/{}", d.os, d.architecture)),
        SharedString::from(d.created.clone()),
        SharedString::from(d.entrypoint.clone()),
        SharedString::from(d.cmd.clone()),
    ])
}
