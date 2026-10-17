//! UI 属性灌入辅助函数
//!
//! pump_and_apply 里的 invoke_from_event_loop 闭包会调这些，
//! 把锁外拿到的数据一次性 set 到 Slint 属性。

use crate::detail_fmt::DrawerData;
use crate::slint_gen::MainWindow;
use slint::{ModelRc, SharedString};

/// 把抽屉内容灌入 Drawer（行列表走 render 缓存模型，避免重建 Repeater）
pub fn apply_drawer(ui: &MainWindow, d: &DrawerData) {
    ui.set_drawer_title(SharedString::from(d.title.as_str()));
    ui.set_drawer_sub(SharedString::from(d.sub.as_str()));
    ui.set_drawer_tabs(d.tabs_visible);
    let detail = crate::render::detail_rows(d.detail.clone());
    let env = crate::render::env_rows(d.env.clone());
    ui.set_detail_rows(detail);
    ui.set_env_rows(env);
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
    let cm = crate::render::containers(crate::ui_rows::build_container_rows(s));
    let im = crate::render::images(crate::ui_rows::build_image_rows(s));
    let nm = crate::render::networks(crate::ui_rows::build_network_rows(s));
    let vm = crate::render::volumes(crate::ui_rows::build_volume_rows(s));
    apply_lists(ui, cm, im, nm, vm, ui.get_log_lines());
}
