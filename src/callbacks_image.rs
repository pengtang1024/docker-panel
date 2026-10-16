//! 镜像操作回调（AppActions global）

use slint::{ComponentHandle, SharedString};

use crate::docker;
use crate::models::Msg;
use crate::slint_gen::*;
use crate::ui_state::SharedState;

pub fn bind_image_actions(ui: &MainWindow, state: &SharedState) {
    let actions = ui.global::<AppActions>();

    // 镜像删除
    {
        let ui_a = ui.as_weak();
        let s = state.clone();
        actions.on_image_remove(move |row_id: SharedString| {
            let full_id = row_id.to_string();
            let Some(ui) = ui_a.upgrade() else { return };
            ui.set_status_text(SharedString::from("删除镜像..."));

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
                        let _ = tx.send(Msg::ActionDone("镜像已删除".into(), Ok("ok".into())));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ActionDone("删除镜像失败".into(), Err(e)));
                    }
                }
            });
        });
    }
}
