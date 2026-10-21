//! Compose 侧栏树回调（选中范围 / 展开收起）
//!
//! 纪律（AGENTS.md 规范 #18）：compose 范围与展开集合是**用户操作过的状态**，
//! 只由本文件（UI 回调，UI 线程）写入 `AppState`；consumer 只读
//! （3s 刷新时按当前范围重建树的计数），绝不回写。
//!
//! 改范围会连带改变台账的过滤结果 → 同批重建台账行 + 概览带，
//! 并递增 `rows_epoch`（让 consumer 手里那批旧行作废）与 `compose_epoch`
//! （让 consumer 手里那批旧树作废）。

use std::sync::atomic::Ordering;

use slint::{ComponentHandle, SharedString, Weak};

use crate::compose_scope::ComposeScope;
use crate::slint_gen::{AppActions, MainWindow};
use crate::ui_state::SharedState;

/// 节点 kind（与 `ui_rows::compose_tree` 常量一致）
const KIND_PROJECT: i32 = 1;
const KIND_SERVICE: i32 = 2;
const KIND_STANDALONE: i32 = 3;

/// 绑定侧栏树回调
pub fn bind(_ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    // —— 选中节点（点标签）——
    {
        let ui_w: Weak<MainWindow> = _ui.as_weak();
        let s = state.clone();
        actions.on_compose_select(
            move |kind: i32, project: SharedString, service: SharedString| {
                let Some(ui) = ui_w.upgrade() else { return };
                let project = project.to_string();
                let service = service.to_string();
                let nodes = {
                    let Ok(mut st) = s.lock() else { return };
                    let next = match kind {
                        KIND_PROJECT => ComposeScope::Project(project.clone()),
                        KIND_SERVICE => ComposeScope::Service {
                            project: project.clone(),
                            service: service.clone(),
                        },
                        KIND_STANDALONE => ComposeScope::Standalone,
                        _ => ComposeScope::All,
                    };
                    // 再点已选中的节点 = 取消选中，回到「全部容器」
                    st.compose.scope = if st.compose.scope == next {
                        ComposeScope::All
                    } else {
                        next
                    };
                    // 选中项目时自动展开该项目（便于继续点服务细分）
                    let expand = match &st.compose.scope {
                        ComposeScope::Project(p) => Some(p.clone()),
                        _ => None,
                    };
                    if let Some(p) = expand {
                        st.compose.expanded.insert(p);
                    }
                    st.compose_epoch.fetch_add(1, Ordering::AcqRel);
                    st.rows_epoch.fetch_add(1, Ordering::AcqRel);
                    st.persist();
                    crate::ui_rows::build_compose_tree(&st)
                };
                ui.set_compose_nodes(crate::render::compose_tree(nodes));
                // 范围变了 → 台账要按新范围过滤（概览带数字口径同批更新），
                // 筛选栏芯片计数也跟着换口径（compose 范围内的分布）
                if let Ok(st) = s.lock() {
                    crate::ui_apply::refresh_lists_now(&ui, &st);
                    let (total, running) = crate::ui_rows::scoped_counts(&st);
                    ui.set_chip_total(total as i32);
                    ui.set_chip_running(running as i32);
                }
            },
        );
    }

    // —— 展开 / 收起项目（点箭头）——
    {
        let ui_w: Weak<MainWindow> = _ui.as_weak();
        let s = state.clone();
        actions.on_compose_toggle(move |project: SharedString| {
            let Some(ui) = ui_w.upgrade() else { return };
            let nodes = {
                let Ok(mut st) = s.lock() else { return };
                st.compose.toggle(project.as_str());
                st.compose_epoch.fetch_add(1, Ordering::AcqRel);
                st.persist();
                crate::ui_rows::build_compose_tree(&st)
            };
            ui.set_compose_nodes(crate::render::compose_tree(nodes));
        });
    }
}
