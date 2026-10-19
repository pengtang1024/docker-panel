//! Drawer 回调绑定（Tab 切换 + 日志工具条：跟随/复制/清空）

use slint::{ComponentHandle, SharedString};

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
                let new_stream = {
                    let Ok(mut st) = s.lock() else { return };
                    if st.sel == SelKind::Container && !st.sel_key.is_empty() {
                        let id = st.sel_key.clone();
                        st.ensure_follow(&id)
                    } else {
                        false
                    }
                };
                // 新建的流没有「正在重连」之类的历史状态，清掉残留提示（保留日志文本）
                if new_stream {
                    crate::ui_apply::clear_log_status(&ui);
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
                st.clear_log_buffer();
            }
            crate::ui_apply::clear_log_panel(&ui);
            ui.set_status_text(SharedString::from(crate::i18n::t("Log cleared")));
        });
    }
    {
        let ui_d = ui.as_weak();
        actions.on_value_copy(move |v: SharedString| {
            let Some(ui) = ui_d.upgrade() else { return };
            let text = v.as_str().to_string();
            let n = text.chars().count();
            let msg = match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                Ok(()) => crate::i18n::trf("Copied {0} chars", &[&n.to_string()]),
                Err(_) => crate::i18n::t("Copy failed"),
            };
            ui.set_status_text(SharedString::from(msg));
        });
    }
}
