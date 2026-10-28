//! 自动更新相关回调：检查 / 安装 / 打开下载页 / 重启 / 两个配置开关。

use slint::{ComponentHandle, SharedString};

use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_update(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let us = ui.global::<UpdateState>();
        us.set_current(SharedString::from(crate::update::CURRENT_VERSION));
        if let Ok(s) = state.lock() {
            us.set_auto_check(s.check_updates);
            us.set_auto_install(s.auto_install);
            us.set_auto_restart(s.auto_restart);
        }
    }

    {
        let ui_q = ui.as_weak();
        let s = state.clone();
        actions.on_update_check(move || {
            let Some(ui) = ui_q.upgrade() else { return };
            {
                let us = ui.global::<UpdateState>();
                us.set_open(true);
                // 已安装完等待重启 / 正在下载安装：只打开弹窗展示状态
                if us.get_restart_needed() || us.get_busy() {
                    return;
                }
                us.set_checking(true);
                us.set_progress(0);
                us.set_status(SharedString::from(crate::i18n::t(
                    "Checking for updates...",
                )));
            }
            if !crate::update::spawn_check(&s, true) {
                let us = ui.global::<UpdateState>();
                us.set_checking(false);
                us.set_status(SharedString::from(crate::i18n::t(
                    "An update task is already running",
                )));
            }
        });
    }

    {
        let ui_q = ui.as_weak();
        actions.on_update_close(move || {
            if let Some(ui) = ui_q.upgrade() {
                ui.global::<UpdateState>().set_open(false);
            }
        });
    }

    {
        let ui_q = ui.as_weak();
        let s = state.clone();
        actions.on_update_install(move || {
            let Some(ui) = ui_q.upgrade() else { return };
            {
                let us = ui.global::<UpdateState>();
                if us.get_busy() || !us.get_installable() {
                    return;
                }
                us.set_busy(true);
                us.set_progress(0);
                us.set_status(SharedString::from(crate::i18n::t("Preparing update...")));
            }
            // spawn_install 失败（没有缓存的检查结果）会通过 Msg::UpdateDone 回填错误
            let _ = crate::update::spawn_install(&s);
        });
    }

    {
        let ui_q = ui.as_weak();
        let s = state.clone();
        actions.on_update_open_page(move || {
            let Some(ui) = ui_q.upgrade() else { return };
            let url = {
                let Ok(st) = s.lock() else { return };
                st.update_info
                    .as_ref()
                    .map(|i| i.page_url.clone())
                    .unwrap_or_else(|| crate::update::RELEASES_PAGE.to_string())
            };
            if let Err(e) = crate::update::install::open_url(&url) {
                crate::ui_apply::status_toast(&ui, &s, &e);
            }
        });
    }

    {
        let ui_q = ui.as_weak();
        let s = state.clone();
        actions.on_update_restart(move || {
            let Some(ui) = ui_q.upgrade() else { return };
            match crate::update::install::restart_current_exe() {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(e) => crate::ui_apply::status_toast(&ui, &s, &e),
            }
        });
    }

    {
        let s = state.clone();
        actions.on_update_set_auto_check(move |checked: bool| {
            if let Ok(mut st) = s.lock() {
                st.check_updates = checked;
                st.persist();
            }
        });
    }

    {
        let s = state.clone();
        actions.on_update_set_auto_install(move |checked: bool| {
            if let Ok(mut st) = s.lock() {
                st.auto_install = checked;
                st.persist();
            }
        });
    }

    {
        let s = state.clone();
        actions.on_update_set_auto_restart(move |checked: bool| {
            if let Ok(mut st) = s.lock() {
                st.auto_restart = checked;
                st.persist();
            }
        });
    }
}
