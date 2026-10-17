//! Drawer 回调绑定（Tab 切换 + 日志工具条：跟随/复制/清空）

use slint::{ComponentHandle, ModelRc, SharedString};

use crate::slint_gen::*;
use crate::ui_state::{SelKind, SharedState};

pub fn bind_drawer(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    {
        let ui_d = ui.as_weak();
        actions.on_drawer_close(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            ui.set_drawer_open(false);
        });
    }
    {
        let ui_d = ui.as_weak();
        let s = state.clone();
        actions.on_drawer_tab(move |t: i32| {
            let Some(ui) = ui_d.upgrade() else { return };
            ui.set_drawer_tab(t);
            // 切到「日志」Tab 时，若当前选中是容器则即时建立日志跟随流
            // （卡片 ⋯→日志 已走 container_logs；此处覆盖用户手动点 Tab 的路径）
            if t == 1 {
                if let Ok(mut st) = s.lock() {
                    if st.sel == SelKind::Container && !st.sel_key.is_empty() {
                        let id = st.sel_key.clone();
                        st.ensure_follow(&id);
                    }
                }
            }
        });
    }
    {
        let ui_d = ui.as_weak();
        let s = state.clone();
        actions.on_log_toggle_follow(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            let next = !ui.get_log_follow();
            ui.set_log_follow(next);
            if let Ok(mut st) = s.lock() {
                st.log_follow = next;
                st.persist();
            }
        });
    }
    {
        let ui_d = ui.as_weak();
        let s = state.clone();
        actions.on_log_copy(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            let text = match s.lock() {
                Ok(st) => st.logs.join("\n"),
                Err(_) => return,
            };
            if text.is_empty() {
                ui.set_status_text(SharedString::from(crate::i18n::t("Log is empty")));
                return;
            }
            let n = text.lines().count();
            let msg = match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                Ok(()) => crate::i18n::trf("Copied {0} log lines", &[&n.to_string()]),
                Err(_) => crate::i18n::t("Copy failed"),
            };
            ui.set_status_text(SharedString::from(msg));
        });
    }
    {
        let ui_d = ui.as_weak();
        let s = state.clone();
        actions.on_log_clear(move || {
            let Some(ui) = ui_d.upgrade() else { return };
            if let Ok(mut st) = s.lock() {
                st.clear_logs();
            }
            let empty: Vec<LogLineRow> = Vec::new();
            ui.set_log_lines(ModelRc::from(empty.as_slice()));
            ui.set_status_text(SharedString::from(crate::i18n::t("Log cleared")));
        });
    }
}
