//! 设置弹窗回调：开关弹窗 / 显式设置主题（0=跟随系统 1=浅色 2=深色）。
//!
//! 从 callbacks.rs 拆出（R2：单文件正文 ≤300 行）。

use slint::ComponentHandle;

use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_settings(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_q = ui.as_weak();
        actions.on_settings_open(move || {
            if let Some(ui) = ui_q.upgrade() {
                ui.global::<SettingsState>().set_open(true);
            }
        });
    }

    {
        let ui_q = ui.as_weak();
        actions.on_settings_close(move || {
            if let Some(ui) = ui_q.upgrade() {
                ui.global::<SettingsState>().set_open(false);
            }
        });
    }

    {
        let ui_q = ui.as_weak();
        let s = state.clone();
        actions.on_set_theme(move |v: i32| {
            let Some(ui) = ui_q.upgrade() else { return };
            if !(0..=2).contains(&v) {
                return;
            }
            ui.set_theme_mode(v);
            if let Ok(mut st) = s.lock() {
                st.theme = v as i64;
                st.persist();
            }
        });
    }
}
