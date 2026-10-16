//! UI 属性灌入辅助函数
//!
//! pump_and_apply 里的 invoke_from_event_loop 闭包会调这些，
//! 把锁外拿到的 SharedString/ModelRc 一次性 set 到 Slint 属性。

use crate::slint_gen::MainWindow;
use crate::ui_state::DetailTuple;
use slint::{ModelRc, SharedString};

/// 把 14 字段 SharedString Vec 灌入右侧镜像面板
pub fn apply_image_detail(ui: &MainWindow, fields: &[SharedString]) {
    if fields.len() < 14 {
        return;
    }
    ui.set_i_short(fields[0].clone());
    ui.set_i_repo(fields[1].clone());
    ui.set_i_created(fields[2].clone());
    ui.set_i_author(fields[3].clone());
    ui.set_i_size(fields[4].clone());
    ui.set_i_platform(fields[5].clone());
    ui.set_i_entry(fields[6].clone());
    ui.set_i_cmd(fields[7].clone());
    ui.set_i_workdir(fields[8].clone());
    ui.set_i_ports(fields[9].clone());
    ui.set_i_volumes(fields[10].clone());
    ui.set_i_labels(fields[11].clone());
    ui.set_i_env(fields[12].clone());
    ui.set_i_id(fields[13].clone());
}

/// 把 18 字段元组灌入 DetailPanel
pub fn apply_detail(ui: &MainWindow, t: &DetailTuple) {
    let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r) = t;
    ui.set_d_name(a.clone());
    ui.set_d_state(b.clone());
    ui.set_d_status(c.clone());
    ui.set_d_image(d.clone());
    ui.set_d_id(e.clone());
    ui.set_d_pid(f.clone());
    ui.set_d_exit(g.clone());
    ui.set_d_created(h.clone());
    ui.set_d_started(i.clone());
    ui.set_d_restart(j.clone());
    ui.set_d_netmode(k.clone());
    ui.set_d_ports(l.clone());
    ui.set_d_mounts(m.clone());
    ui.set_d_networks(n.clone());
    ui.set_d_entry(o.clone());
    ui.set_d_cmd(p.clone());
    ui.set_d_workdir(q.clone());
    ui.set_d_labels(r.clone());
}

/// 把 ModelRc 列表灌入窗口
pub fn apply_lists(
    ui: &MainWindow,
    cm: ModelRc<crate::slint_gen::ContainerRow>,
    im: ModelRc<crate::slint_gen::ImageRow>,
    nm: ModelRc<crate::slint_gen::NetworkRow>,
    vm: ModelRc<crate::slint_gen::VolumeRow>,
    lm: ModelRc<crate::slint_gen::LogLineRow>,
) {
    ui.set_container_rows(cm);
    ui.set_image_rows(im);
    ui.set_network_rows(nm);
    ui.set_volume_rows(vm);
    ui.set_log_lines(lm);
}
