//! UI 属性灌入辅助函数
//!
//! pump_and_apply 里的 invoke_from_event_loop 闭包会调这些，
//! 把锁外拿到的 SharedString/ModelRc 一次性 set 到 Slint 属性。

use crate::slint_gen::MainWindow;
use crate::ui_state::DetailTuple;
use slint::{ModelRc, SharedString};

/// 把镜像详情字段灌入 Drawer
pub fn apply_image_detail(ui: &MainWindow, fields: &[SharedString]) {
    if fields.len() < 6 {
        return;
    }
    ui.set_i_repo(fields[0].clone());
    ui.set_i_size(fields[1].clone());
    ui.set_i_layers(fields[2].clone());
    ui.set_i_created(fields[3].clone());
    ui.set_i_entry(fields[4].clone());
    ui.set_i_cmd(fields[5].clone());
}

/// 把 18 字段元组灌入 Drawer 详情
pub fn apply_detail(ui: &MainWindow, t: &DetailTuple) {
    let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r) = t;
    ui.set_d_name(a.clone());
    ui.set_d_state(b.clone());
    ui.set_d_status(c.clone());
    ui.set_d_image(d.clone());
    ui.set_d_id(e.clone());
    ui.set_d_pid(f.clone());
    ui.set_d_disk(g.clone());
    ui.set_d_cpu(h.clone());
    ui.set_d_mem(i.clone());
    ui.set_d_restart(j.clone());
    ui.set_d_created(k.clone());
    ui.set_d_started(l.clone());
    ui.set_d_netmode(m.clone());
    ui.set_d_ports(n.clone());
    ui.set_d_mounts(o.clone());
    ui.set_d_networks(p.clone());
    ui.set_d_entry(q.clone());
    ui.set_d_cmd(r.clone());
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

/// UI 线程内按当前 AppState 立即重建四个资源列表（搜索/筛选/排序回调的即时反馈）。
/// 网络/卷不在 3s 定时刷新范围内，不主动重建则过滤永不生效。
/// 日志模型原样回传，避免清空日志面板。
pub fn refresh_lists_now(ui: &MainWindow, s: &crate::ui_state::AppState) {
    let cm: ModelRc<crate::slint_gen::ContainerRow> =
        ModelRc::from(crate::ui_rows::build_container_rows(s).as_slice());
    let im: ModelRc<crate::slint_gen::ImageRow> =
        ModelRc::from(crate::ui_rows::build_image_rows(s).as_slice());
    let nm: ModelRc<crate::slint_gen::NetworkRow> =
        ModelRc::from(crate::ui_rows::build_network_rows(s).as_slice());
    let vm: ModelRc<crate::slint_gen::VolumeRow> =
        ModelRc::from(crate::ui_rows::build_volume_rows(s).as_slice());
    apply_lists(ui, cm, im, nm, vm, ui.get_log_lines());
}
