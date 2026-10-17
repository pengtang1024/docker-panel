//! 容器抽屉内容（详情 5 节 + 环境 4 节），对齐 prototype detailHTML/envHTML

use crate::i18n::{t, trf};
use crate::models::{ContainerDetail, ContainerInfo};
use crate::slint_gen::DetailPair;
use crate::ui_state::AppState;

use super::{chip, dash, head, join_or, kbox, kv, or_dash, DrawerData};

pub(crate) fn build(s: &AppState) -> Option<DrawerData> {
    let d = s.detail.as_ref()?;
    let c = super::container_of(s, &d.id);
    Some(DrawerData {
        title: d.name.clone(),
        sub: or_dash(&d.image),
        tabs_visible: true,
        detail: detail_rows(d, c),
        env: env_rows(d),
    })
}

fn detail_rows(d: &ContainerDetail, c: Option<&ContainerInfo>) -> Vec<DetailPair> {
    let running = d.state == "running";
    let state_text = if running { t("Running") } else { t("Stopped") };

    let mut rows = vec![
        head(&t("Overview")),
        kv(
            &t("Status"),
            format!("{state_text} · {}", or_dash(&d.status)),
            false,
        ),
        kv("PID", or_dash(&d.pid.to_string()), true),
        kv("ID", or_dash(&d.id), true),
        kv(
            &t("Disk"),
            c.map(|c| super::format_bytes(c.size_rw))
                .filter(|s| s != "0 B")
                .unwrap_or_else(dash),
            true,
        ),
        head(&t("Resources")),
    ];

    let cpu = match c {
        Some(c) if running => format!("{:.1}%", c.cpu_percent),
        _ => dash(),
    };
    let mem = match c {
        Some(c) if running => format!("{} · {}", super::format_bytes(c.mem_usage), mem_limit(c)),
        _ => dash(),
    };
    rows.push(kv("CPU", cpu, false));
    rows.push(kv(&t("Memory"), mem, false));
    rows.push(kv(
        &t("Restart Policy"),
        or_dash(&d.restart_policy),
        false,
    ));
    rows.push(kv(
        &t("Restart Count"),
        trf("{0} times", &[&d.restarts.to_string()]),
        false,
    ));

    rows.push(head(&t("Time")));
    rows.push(kv(&t("Created"), or_dash(&d.created), false));
    rows.push(kv(&t("Started"), or_dash(&d.started_at), false));

    rows.push(head(&t("Network")));
    rows.push(kv(&t("Mode"), or_dash(&d.network_mode), false));
    rows.push(kv(&t("Ports"), join_or(&d.ports, " · "), true));
    rows.push(kv(&t("Networks"), join_or(&d.networks, ", "), false));
    rows.push(kv(&t("Mounts"), join_or(&d.mounts, " · "), true));

    rows.push(head(&t("Config")));
    rows.push(kbox(&t("Entrypoint"), or_dash(&d.entrypoint)));
    rows.push(kbox(&t("Command"), or_dash(&d.command)));
    rows.push(kbox("Labels", labels_text(&d.labels)));
    rows
}

fn env_rows(d: &ContainerDetail) -> Vec<DetailPair> {
    let mut rows = vec![head(&t("Environment Variables"))];
    if d.env.is_empty() {
        rows.push(chip("", t("None")));
    } else {
        for e in &d.env {
            match e.split_once('=') {
                Some((k, v)) => rows.push(chip(k, v.to_string())),
                None => rows.push(chip("", e.clone())),
            }
        }
    }

    rows.push(head(&t("Port Mappings")));
    if d.ports.is_empty() {
        rows.push(chip("", t("No ports")));
    } else {
        for p in &d.ports {
            rows.push(chip("", p.clone()));
        }
    }

    rows.push(head(&t("Networks")));
    if d.networks.is_empty() {
        rows.push(chip("", t("None")));
    } else {
        for n in &d.networks {
            rows.push(chip("", n.clone()));
        }
    }

    rows.push(head(&t("Mounts")));
    rows.push(kbox("", join_or(&d.mounts, "\n")));
    rows
}

/// 内存上限：0 表示无限制（对齐 prototype 的 unlimited 文案）
fn mem_limit(c: &ContainerInfo) -> String {
    if c.mem_limit == 0 {
        t("unlimited")
    } else {
        super::format_bytes(c.mem_limit)
    }
}

fn labels_text(labels: &[(String, String)]) -> String {
    if labels.is_empty() {
        dash()
    } else {
        labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
