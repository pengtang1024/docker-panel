//! 容器 Tab：品牌头 + 工具栏 + 容器表格（搜索/多选/排序/列显隐/compose 分组/右键菜单）

mod header;
mod prep;
mod row;
mod table;

use super::{
    cmp_container, column_menu, format_bytes, state_label, ActionKind, Color32, Column, Confirm,
    ContainerInfo, CornerRadius, CtlCol, DvApp, Filter, FontId, HashSet, RichText, RowAction,
    ScrollArea, Stroke, Tab, TableBuilder, TableOutcome, ACCENT, AMBER, BLUE, CYAN, GREEN,
    PANEL_BG, RED, SURFACE_BG, TEXT_DIM,
};
use crate::widgets::{
    check_box, chip, fold_arrow, header_cell, ic, icon_button, menu_item, nav_tab, right_label,
    search_box, sort_header, NEUTRAL,
};

use table::container_table;

/// 表格里的一行：compose 组头，或指向下标 rows[i] 的容器行
enum DispRow {
    /// compose 项目组头（项目名, 成员数）
    Group { project: String, count: usize },
    /// 容器行（下标指向过滤排序后的 rows）
    Container(usize),
}

impl DvApp {
    /// 容器 Tab
    pub(super) fn ui_containers(&mut self, ui: &mut egui::Ui) {
        // 工具条：搜索 + 筛选 + 分组 + 右侧操作
        // 闭包内只收集局部 flag，闭包外统一执行，规避 egui 借用冲突
        let mut need_refresh = false;
        let mut batch_kind: Option<ActionKind> = None;
        let mut batch_delete = false;
        let mut clear_sel = false;
        let selected_count = self.selected_containers.len();
        let prev_filter = self.filter;

        ui.horizontal(|ui| {
            search_box(
                ui,
                &mut self.container_query,
                "搜索名称 / ID / 镜像 / 端口…",
            );
            ui.label(RichText::new("筛选").weak());
            ui.selectable_value(&mut self.filter, Filter::Running, "仅正在运行");
            ui.selectable_value(&mut self.filter, Filter::All, "全部");
            ui.separator();
            ui.toggle_value(&mut self.group_compose, "Compose 分组")
                .on_hover_text("把 compose 启动的容器按项目收进二级分组");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, ic::SWEEP, "简易清理…", AMBER, true) {
                    self.cleanup_open = true;
                }
                if icon_button(ui, ic::REFRESH, "刷新列表", NEUTRAL, true) {
                    need_refresh = true;
                }
                // 自定义显示列
                ui.menu_button(format!("{}  显示列", ic::COLUMNS), |ui| {
                    column_menu(
                        ui,
                        &CtlCol::ALL,
                        &mut self.ctl_cols,
                        super::columns::CtlCol::label,
                    );
                })
                .response
                .on_hover_text("自定义显示列");
                // 首屏加载才有转圈，平时刷新不占位，避免工具条来回抖
                if self.containers_loading && self.containers.is_empty() {
                    ui.spinner();
                }
            });
        });

        // 筛选条件参与 Docker 查询参数，改动后需立即重新拉取
        if self.filter != prev_filter {
            need_refresh = true;
        }

        // 批量操作条：有选中项才出现
        if selected_count > 0 {
            ui.horizontal(|ui| {
                ui.colored_label(BLUE, format!("已选 {selected_count} 项"));
                if icon_button(ui, ic::PLAY, "批量启动", GREEN, true) {
                    batch_kind = Some(ActionKind::Start);
                }
                if icon_button(ui, ic::STOP, "批量停止", RED, true) {
                    batch_kind = Some(ActionKind::Stop);
                }
                if icon_button(ui, ic::RESTART, "批量重启", AMBER, true) {
                    batch_kind = Some(ActionKind::Restart);
                }
                if icon_button(ui, ic::DELETE, "批量删除…", RED, true) {
                    batch_delete = true;
                }
                if icon_button(ui, ic::CLOSE, "清除选择", NEUTRAL, true) {
                    clear_sel = true;
                }
            });
        }
        ui.add_space(6.0);

        if need_refresh {
            self.refresh_containers();
        }
        if clear_sel {
            self.selected_containers.clear();
        }

        // 过滤 + 排序 + compose 分组
        let (rows, disp, q) = self.build_disp();

        // 批量动作：选中集合 ∩ 当前可见行（一次收集，启动/停止/重启/删除共用）
        let batch_items: Vec<(String, String)> = if batch_kind.is_none() && !batch_delete {
            Vec::new()
        } else {
            rows.iter()
                .filter(|c| self.selected_containers.contains(&c.id))
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect()
        };
        if let Some(kind) = batch_kind {
            if !batch_items.is_empty() {
                self.spawn_batch_action(kind, batch_items);
            }
            self.selected_containers.clear();
        } else if batch_delete && !batch_items.is_empty() {
            self.confirm = Some(Confirm::DeleteContainers { items: batch_items });
        }

        let all_selected = !rows.is_empty()
            && rows
                .iter()
                .all(|c| self.selected_containers.contains(&c.id));
        let selected = self.selected_containers.clone();
        let cols = self.ctl_cols.clone();
        let sort = self.ctl_sort;

        let TableOutcome {
            select_all,
            sort_click,
            toggles,
            actions,
        } = container_table(
            ui,
            &rows,
            &disp,
            &selected,
            &cols,
            sort,
            all_selected,
            &mut self.collapsed_groups,
        );

        // 点表头：同列翻转方向，换列则按升序
        if let Some(col) = sort_click {
            if col == self.ctl_sort.0 {
                self.ctl_sort.1 = !self.ctl_sort.1;
            } else {
                self.ctl_sort = (col, true);
            }
        }

        // 表格渲染完毕后统一应用：表头全选 / 逐行切换 / 行动作
        if let Some(select) = select_all {
            for c in &rows {
                if select {
                    self.selected_containers.insert(c.id.clone());
                } else {
                    self.selected_containers.remove(&c.id);
                }
            }
        }
        for id in toggles {
            if !self.selected_containers.insert(id.clone()) {
                self.selected_containers.remove(&id);
            }
        }
        for action in actions {
            match action {
                RowAction::Logs(id, name) => self.open_log(id, name),
                RowAction::Stats(id, name) => self.open_resource(id, name),
                RowAction::Start(id, name) => self.spawn_action(ActionKind::Start, id, name),
                RowAction::Stop(id, name) => self.spawn_action(ActionKind::Stop, id, name),
                RowAction::Restart(id, name) => self.spawn_action(ActionKind::Restart, id, name),
                RowAction::DeleteContainer(id, name) => {
                    self.confirm = Some(Confirm::DeleteContainer { id, name });
                }
                RowAction::DeleteImage(name, tag) => {
                    self.confirm = Some(Confirm::DeleteImage { name, tag });
                }
            }
        }

        if rows.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    if !q.is_empty() {
                        ui.label(RichText::new("没有匹配的容器").size(15.0).weak());
                        ui.label(RichText::new("换个关键词，或点搜索框右侧的 × 清空").weak());
                    } else if self.containers.is_empty() {
                        ui.label(RichText::new("没有容器").size(15.0).weak());
                        ui.label(RichText::new("启动容器后会显示在这里").weak());
                    } else {
                        ui.label(RichText::new("没有符合条件的容器").size(15.0).weak());
                        ui.label(RichText::new("试试切换筛选条件，或点击右上角刷新").weak());
                    }
                });
            });
        }
    }
}
