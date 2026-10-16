//! UI 属性灌入辅助函数
//!
//! pump_and_apply 里的 invoke_from_event_loop 闭包会调这些，
//! 把锁外拿到的 SharedString/ModelRc 一次性 set 到 Slint 属性。

use crate::slint_gen::MainWindow;
use crate::ui_state::DetailTuple;
use slint::ModelRc;

/// 把 17 字段元组灌入 DetailPanel
pub fn apply_detail(ui: &MainWindow, t: &DetailTuple) {
    let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q) = t;
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
