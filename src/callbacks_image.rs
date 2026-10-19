//! 镜像操作回调

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::i18n::trf;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::{SelKind, SharedState};

pub fn bind_image_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();
    // 镜像卡片点击 → 打开抽屉 + inspect
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_image_select(move |row_id: SharedString| {
            let name = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_drawer_open(true);
            ui.set_drawer_tab(0);
            if let Ok(mut st) = s.lock() {
                st.sel = SelKind::Image;
                st.sel_key = name.clone();
                st.clear_logs();
                st.request_image_detail(name);
            }
        });
    }

    // 镜像卡片「运行」→ 状态栏提示（按原型：不真启动容器）
    {
        let ui_a = ui.as_weak();
        actions.on_image_run(move |row_id: SharedString| {
            let name = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from(trf("Run {0}", &[&name]).as_str()));
        });
    }

    // 镜像删除
    {
        let s = state.clone();
        actions.on_image_remove(move |row_id: SharedString| {
            let full_id = row_id.to_string();
            let (name, tag) = match full_id.rsplit_once(':') {
                Some((n, t)) if !n.is_empty() && !t.contains('/') => (n.to_string(), t.to_string()),
                _ => (full_id, "".to_string()),
            };
            let (rt_handle, tx) = {
                let Ok(st) = s.lock() else { return };
                (st.rt.handle().clone(), st.tx.clone())
            };
            rt_handle.spawn(async move {
                match docker::remove_image(&name, &tag).await {
                    Ok(_) => {
                        let _ = tx.send(Msg::ActionDone("Image removed".into(), Ok("ok".into())));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("Failed to remove image".into(), Err(e)));
                    }
                }
            });
        });
    }
}
