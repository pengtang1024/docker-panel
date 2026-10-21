//! UI 回调绑定

pub use super::callbacks_container::bind_container_actions;
use super::callbacks_drawer::bind_drawer;
pub use super::callbacks_image::bind_image_actions;
pub use super::callbacks_netvol::bind_netvol_actions;

use std::sync::atomic::Ordering;

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::{Msg, SortField};
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_callbacks(ui: &MainWindow, state: &SharedState) {
    restore_config(ui, state);
    let actions = ui.global::<AppActions>();
    bind_top_bar(ui, &actions, state);
    bind_sub_bar(ui, &actions, state);
    bind_pull_dialog(ui, &actions, state);
    bind_cleanup_dialog(ui, &actions, state);
    bind_drawer(ui, &actions, state);
    bind_container_actions(ui, state);
    bind_image_actions(ui, state);
    bind_netvol_actions(ui, state);
}

/// 启动时把持久化的配置灌回 Slint 属性
fn restore_config(ui: &MainWindow, state: &SharedState) {
    let Ok(s) = state.lock() else {
        return;
    };
    ui.set_theme_mode(s.theme as i32);
    ui.set_view(s.view as i32);
    ui.set_filter(SharedString::from(s.filter.as_str()));
    ui.set_sort_field(s.sort_field.to_ui_index() as i32);
    ui.set_sort_desc(s.sort_desc);
    ui.set_search_q(SharedString::from(s.container_query.as_str()));
    ui.set_log_follow(s.log_follow);

    // 窗口 show、事件循环跑起来后再引导一次键盘焦点：
    // init 阶段的 force-focus 在部分后端会被窗口显示流程清掉，
    // 而 Slint 没有持久焦点元素时不派发任何按键事件（全局快捷键失效）。
    // run() 之前排队的 invoke 会被 winit 丢弃，故延时到循环启动后投递。
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_boot_tick(1);
            }
        });
    });
}

fn bind_top_bar(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let s = state.clone();
        actions.on_refresh(move || {
            if let Ok(mut st) = s.lock() {
                st.refresh();
            }
        });
    }
    {
        let s = state.clone();
        let ui_q = ui.as_weak();
        actions.on_search(move |text: SharedString| {
            let Some(ui) = ui_q.upgrade() else { return };
            {
                let q = text.to_string();
                let Ok(mut st) = s.lock() else { return };
                st.container_query = q.clone();
                st.image_query = q;
                st.rows_epoch.fetch_add(1, Ordering::AcqRel);
                st.persist();
            }
            // 网络/卷无定时刷新，必须立即重建，否则过滤看不到效果
            if let Ok(st) = s.lock() {
                crate::ui_apply::refresh_lists_now(&ui, &st);
            }
        });
    }
    {
        let ui_p = ui.as_weak();
        actions.on_pull(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(true);
            ui.set_pull_input(SharedString::from(""));
            ui.set_pull_busy(false);
        });
    }
    {
        let ui_c = ui.as_weak();
        let s = state.clone();
        actions.on_cleanup(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            // 打开前按当前列表回填各类候选数量（弹窗 .cnt 展示）
            if let Ok(st) = s.lock() {
                let (a, b, c, d) = st.cleanup_counts();
                ui.set_cleanup_cnt_stopped(SharedString::from(a.to_string().as_str()));
                ui.set_cleanup_cnt_dangling(SharedString::from(b.to_string().as_str()));
                ui.set_cleanup_cnt_unused_net(SharedString::from(c.to_string().as_str()));
                ui.set_cleanup_cnt_unused_vol(SharedString::from(d.to_string().as_str()));
            }
            ui.set_cleanup_open(true);
        });
    }
    {
        let ui_t = ui.as_weak();
        let s = state.clone();
        actions.on_toggle_theme(move || {
            let Some(ui) = ui_t.upgrade() else { return };
            let next = if ui.get_theme_mode() >= 2 {
                0
            } else {
                ui.get_theme_mode() + 1
            };
            ui.set_theme_mode(next);
            if let Ok(mut st) = s.lock() {
                st.theme = next as i64;
                st.persist();
            }
        });
    }
}

fn bind_sub_bar(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let s = state.clone();
        let ui_f = ui.as_weak();
        actions.on_set_view(move |v: i32| {
            // 必须同步更新 UI 属性，否则画面冻结在启动恢复值（点击/快捷键切视图均失效）
            if let Some(ui) = ui_f.upgrade() {
                ui.set_view(v);
            }
            if let Ok(mut st) = s.lock() {
                st.view = v as i64;
                st.persist();
            }
        });
    }
    {
        let s = state.clone();
        let ui_f = ui.as_weak();
        actions.on_set_filter(move |f: SharedString| {
            let Some(ui) = ui_f.upgrade() else { return };
            ui.set_filter(f.clone());
            {
                let key = f.to_string();
                let Ok(mut st) = s.lock() else { return };
                st.filter = key;
                st.rows_epoch.fetch_add(1, Ordering::AcqRel);
                st.persist();
            }
            if let Ok(st) = s.lock() {
                crate::ui_apply::refresh_lists_now(&ui, &st);
            }
        });
    }
    {
        // Slint 索引：0=Name 1=Cpu 2=Mem 3=Disk 4=Status
        let s = state.clone();
        let ui_f = ui.as_weak();
        actions.on_set_sort(move |f: i32| {
            let Some(ui) = ui_f.upgrade() else { return };
            let (field_idx, desc) = {
                let new_field = SortField::from_ui_index(f as i64);
                let Ok(mut st) = s.lock() else { return };
                // 切换到新字段默认降序；同字段重复点击则翻转方向
                st.sort_desc = st.sort_field != new_field || !st.sort_desc;
                st.sort_field = new_field;
                st.rows_epoch.fetch_add(1, Ordering::AcqRel);
                st.persist();
                (st.sort_field.to_ui_index() as i32, st.sort_desc)
            };
            // 排序状态由回调独占写入（与 filter/view 一致）：consumer 心跳拿到的是
            // 旧快照，回写会把箭头打回旧方向 —— 排序状态与上下箭头偶发不一致的根因
            ui.set_sort_field(field_idx);
            ui.set_sort_desc(desc);
            if let Ok(st) = s.lock() {
                crate::ui_apply::refresh_lists_now(&ui, &st);
            }
        });
    }
    {
        let s = state.clone();
        let ui_f = ui.as_weak();
        actions.on_set_sort_dir(move || {
            let Some(ui) = ui_f.upgrade() else { return };
            let desc = {
                let Ok(mut st) = s.lock() else { return };
                st.sort_desc = !st.sort_desc;
                st.rows_epoch.fetch_add(1, Ordering::AcqRel);
                st.persist();
                st.sort_desc
            };
            // 同上：方向箭头只由回调写，consumer 不再回写快照值
            ui.set_sort_desc(desc);
            if let Ok(st) = s.lock() {
                crate::ui_apply::refresh_lists_now(&ui, &st);
            }
        });
    }
}

fn bind_pull_dialog(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_p = _ui.as_weak();
        actions.on_pull_close(move || {
            let Some(ui) = ui_p.upgrade() else { return };
            ui.set_pull_open(false);
        });
    }
    {
        let ui_p = _ui.as_weak();
        let s = state.clone();
        actions.on_pull_submit(move |spec: SharedString| {
            let Some(ui) = ui_p.upgrade() else { return };
            let text = spec.to_string();
            if text.trim().is_empty() {
                ui.set_status_text(SharedString::from(crate::i18n::t(
                    "Please enter an image name",
                )));
                return;
            }
            ui.set_pull_busy(true);
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                let tx_clone = tx.clone();
                let result = docker::pull_image(&text, move |progress| {
                    let _ = tx_clone.send(Msg::PullProgress(progress));
                })
                .await;
                let _ = tx.send(Msg::PullDone(result));
            });
        });
    }
}

fn bind_cleanup_dialog(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_c = _ui.as_weak();
        actions.on_cleanup_close(move || {
            let Some(ui) = ui_c.upgrade() else { return };
            ui.set_cleanup_open(false);
        });
    }
    {
        let ui_c = _ui.as_weak();
        let s = state.clone();
        actions.on_cleanup_submit(
            move |stopped: bool, dangling: bool, unused_net: bool, unused_vol: bool| {
                let Some(ui) = ui_c.upgrade() else { return };
                ui.set_cleanup_open(false);
                if !stopped && !dangling && !unused_net && !unused_vol {
                    ui.set_status_text(SharedString::from(crate::i18n::t(
                        "Select at least one item",
                    )));
                    return;
                }
                let (rt_handle, tx) = {
                    let Ok(st) = s.lock() else { return };
                    (st.rt.handle().clone(), st.tx.clone())
                };
                rt_handle.spawn(async move {
                    let result = docker::cleanup(stopped, dangling, unused_net, unused_vol).await;
                    let _ = tx.send(Msg::CleanupDone(result));
                });
            },
        );
    }
}
