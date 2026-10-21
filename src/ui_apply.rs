//! UI 属性灌入辅助函数
//!
//! consumer 的 `invoke_from_event_loop` 闭包会调这些，
//! 把锁外拿到的数据一次性 set 到 Slint 属性。

use std::time::Duration;

use crate::detail_fmt::DrawerData;
use crate::slint_gen::MainWindow;
use crate::ui_state::SharedState;
use slint::{ComponentHandle, ModelRc, SharedString};

/// 操作结果提示的停留时长：到期后状态栏恢复 "Ready"（toast 语义）
pub const TOAST_DURATION: Duration = Duration::from_secs(2);

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
) {
    ui.set_container_rows(cm);
    ui.set_image_rows(im);
    ui.set_network_rows(nm);
    ui.set_volume_rows(vm);
}

/// UI 线程内按当前 AppState 立即重建四个资源列表（搜索/筛选/排序回调的即时反馈）。
/// 网络/卷不在 3s 定时刷新范围内，不主动重建则过滤永不生效。
pub fn refresh_lists_now(ui: &MainWindow, s: &crate::ui_state::AppState) {
    // 台账行与概览带同批产出：概览带数字口径 = 当前**可见**的容器集合
    let (rows, summary, visible) = crate::ui_rows::build_container_ledger(s);
    let cm = crate::render::containers(rows);
    let im = crate::render::images(crate::ui_rows::build_image_rows(s));
    let nm = crate::render::networks(crate::ui_rows::build_network_rows(s));
    let vm = crate::render::volumes(crate::ui_rows::build_volume_rows(s));
    apply_lists(ui, cm, im, nm, vm);
    ui.set_ledger_summary(summary);
    // 筛选栏芯片计数：可见容器集合的分布（与台账行数同口径）
    ui.set_chip_total(visible.0 as i32);
    ui.set_chip_running(visible.1 as i32);
}

/// 清空整个日志面板（日志行 + 状态提示）。
///
/// 切换选中资源（容器 / 镜像 / 网络 / 卷）或按 Clear 时调用：
/// 日志上下文已经变了，旧行与旧提示都不再属于当前对象。
pub fn clear_log_panel(ui: &MainWindow) {
    ui.set_log_rows(crate::render::clear_logs());
    ui.set_log_status(SharedString::from(""));
}

/// 只清空状态提示，保留日志内容。
///
/// 同一容器重建日志流时调用：流是新的，没有「正在重连」之类的历史状态，
/// 但已缓冲的日志内容仍然有效。
pub fn clear_log_status(ui: &MainWindow) {
    ui.set_log_status(SharedString::from(""));
}

/// 状态栏 toast：写入操作结果，`TOAST_DURATION` 后自动恢复 "Ready"。
///
/// 与 consumer 的 toast 语义一致，但用于「回调直接写状态栏」的场景
/// （复制日志 / 清空日志 / 复制字段值）—— 这些动作不走 Msg 通道，
/// 状态文本会一直停在结果上，看起来像卡住。
pub fn status_toast(ui: &MainWindow, state: &SharedState, text: &str) {
    ui.set_status_text(SharedString::from(text));
    let rt = {
        let Ok(s) = state.lock() else { return };
        s.rt.handle().clone()
    };
    let ui_w = ui.as_weak();
    let owned = text.to_string();
    rt.spawn(async move {
        tokio::time::sleep(TOAST_DURATION).await;
        slint::invoke_from_event_loop(move || {
            let Some(ui) = ui_w.upgrade() else { return };
            // 期间被别的消息改过（如新的操作结果 / 错误）就不覆盖
            if ui.get_status_text().as_str() == owned {
                ui.set_status_text(SharedString::from(crate::i18n::t("Ready")));
            }
        })
        .ok();
    });
}
