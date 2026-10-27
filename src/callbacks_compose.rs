//! 台账内联树回调（分组的展开 / 收起 + 组操作）
//!
//! 纪律（AGENTS.md 规范 #18）：分组的收起状态是**用户操作过的状态**——
//! 只由本文件（UI 回调，UI 线程）写入 `AppState`；consumer 只读
//! （3s 刷新时按当前展开状态重建树），绝不回写。
//!
//! 展开 / 收起会改变**可见容器集合** → 台账行、概览带、筛选栏芯片计数
//! 三者必须同批重建（它们的口径都是「可见集合」）。
//!
//! 组操作（规范 #25 延伸）：分组带上的 3 个按钮与容器行同构，只是动作
//! **统一作用于组内全部成员**（成员口径 = 当前筛选后的容器，与台账可见行一致）。

use std::sync::atomic::Ordering;

use slint::{ComponentHandle, SharedString, Weak};

use super::callbacks_container::{spawn_container_op, OpKind};
use crate::i18n::{t, trf};
use crate::slint_gen::{AppActions, MainWindow};
use crate::ui_rows::{group_label, group_members, GroupMember};
use crate::ui_state::{SelKind, SharedState};

/// 绑定内联树回调
pub fn bind(ui: &MainWindow, actions: &AppActions, state: &SharedState) {
    let ui_w: Weak<MainWindow> = ui.as_weak();
    let s = state.clone();
    // 分组键：`project:<项目名>` / `svc:<项目名>/<服务名>`
    actions.on_compose_toggle(move |key: SharedString| {
        let Some(ui) = ui_w.upgrade() else { return };
        let (rows, summary, visible) = {
            let Ok(mut st) = s.lock() else { return };
            let key = key.to_string();
            // 存的是「收起」的分组键：已在集合里 = 收起 → 移除；否则加入
            if st.compose_collapsed.contains(&key) {
                st.compose_collapsed.remove(&key);
            } else {
                st.compose_collapsed.insert(key);
            }
            // 行模型作废：consumer 手里那批行是旧展开状态建的
            st.rows_epoch.fetch_add(1, Ordering::AcqRel);
            st.persist();
            crate::ui_rows::build_container_ledger(&st)
        };
        ui.set_container_rows(crate::render::containers(rows));
        ui.set_ledger_summary(summary);
        ui.set_chip_total(visible.0 as i32);
        ui.set_chip_running(visible.1 as i32);
    });

    // —— 组操作：启动 / 停止 / 重启 / 删除（统一作用于组内全部成员）——
    // 启动全组：只启动**没在运行**的成员（对运行中的成员再 start 是空操作，
    // 但会白白刷出一堆 ActionDone 提示，噪音大）
    {
        let s = state.clone();
        actions.on_compose_start(move |key: SharedString| {
            for m in group_members_locked(&s, key.as_str()) {
                if !m.running {
                    spawn_container_op(&s, &m.id, OpKind::Start);
                }
            }
        });
    }
    // 停止全组：只停运行中的成员（停止已停止的容器会报错）
    {
        let s = state.clone();
        actions.on_compose_stop(move |key: SharedString| {
            for m in group_members_locked(&s, key.as_str()) {
                if m.running {
                    spawn_container_op(&s, &m.id, OpKind::Stop);
                }
            }
        });
    }
    // 重启全组：只重启运行中的成员（与容器行「重启」按钮口径一致，
    // 停止的容器应该走启动而不是重启）
    {
        let s = state.clone();
        actions.on_compose_restart(move |key: SharedString| {
            for m in group_members_locked(&s, key.as_str()) {
                if m.running {
                    spawn_container_op(&s, &m.id, OpKind::Restart);
                }
            }
        });
    }
    // 删除全组：全部成员
    {
        let s = state.clone();
        actions.on_compose_remove(move |key: SharedString| {
            for m in group_members_locked(&s, key.as_str()) {
                spawn_container_op(&s, &m.id, OpKind::Remove);
            }
        });
    }

    // —— 合并日志：组内每个运行中成员一条流，共享 epoch，行首带 `[容器名]` ——
    {
        let ui_w = ui.as_weak();
        let s = state.clone();
        actions.on_compose_logs(move |key: SharedString| {
            let Some(ui) = ui_w.upgrade() else { return };
            let key = key.to_string();
            let targets: Vec<(String, String)> = group_members_locked(&s, &key)
                .into_iter()
                .filter(|m| m.running)
                .map(|m| (m.id, format!("[{}]", m.name)))
                .collect();
            if targets.is_empty() {
                crate::ui_apply::status_toast(&ui, &s, &t("No running containers in this group"));
                return;
            }
            let label = group_label(&key);
            let merged = trf("{0} containers merged", &[&targets.len().to_string()]);

            // 抽屉切到「只展示日志」模式：隐藏详情 / 环境 Tab
            // （分组没有单个容器的详情，切过去只会看到上一个容器的残留内容）
            crate::ui_apply::clear_log_panel(&ui);
            ui.set_drawer_open(true);
            ui.set_drawer_tab(1);
            ui.set_drawer_log_only(true);
            ui.set_drawer_title(SharedString::from(label.as_str()));
            ui.set_drawer_sub(SharedString::from(merged.as_str()));

            let new_stream = match s.lock() {
                Ok(mut st) => {
                    st.sel = SelKind::Group;
                    st.sel_key = key.clone();
                    // 切到分组日志：旧缓冲与旧流都属于别的对象，全部作废
                    st.clear_logs();
                    st.ensure_group_follow(&key, targets)
                }
                Err(_) => false,
            };
            if new_stream {
                crate::ui_apply::clear_log_status(&ui);
            }
        });
    }
}

/// 锁内取分组成员（口径 = 当前筛选后的容器）
fn group_members_locked(s: &SharedState, key: &str) -> Vec<GroupMember> {
    match s.lock() {
        Ok(st) => group_members(&st, key),
        Err(_) => Vec::new(),
    }
}
