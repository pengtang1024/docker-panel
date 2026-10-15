//! 镜像 Tab：工具栏 + 拉取输入 + 镜像表格（搜索/多选/排序/列显隐/右键菜单）

mod table;

use super::{
    cmp_image, column_menu, format_age, format_bytes, Column, Confirm, DvApp, HashSet, ImageInfo,
    ImgCol, RichText, RowAction, ScrollArea, TableBuilder, TableOutcome, ACCENT, AMBER, BLUE, RED,
    SURFACE_BG,
};
use crate::widgets::{
    check_box, header_cell, ic, icon_button, menu_item, right_label, search_box, sort_header,
    NEUTRAL,
};

use table::image_table;

impl DvApp {
    /// 镜像 Tab
    pub(super) fn ui_images(&mut self, ui: &mut egui::Ui) {
        let mut need_refresh = false;
        let mut need_pull = false;
        let mut batch_delete = false;
        let mut clear_sel = false;
        let selected_count = self.selected_images.len();

        // 工具条第 1 行：搜索 + 右侧刷新/清理
        ui.horizontal(|ui| {
            search_box(ui, &mut self.image_query, "搜索名称 / Tag / ID…");
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
                        &ImgCol::ALL,
                        &mut self.img_cols,
                        super::columns::ImgCol::title,
                    );
                })
                .response
                .on_hover_text("自定义显示列");
                if self.images_loading && self.images.is_empty() {
                    ui.spinner();
                }
            });
        });

        // 工具条第 2 行：拉取输入 + 图标拉取按钮
        ui.horizontal(|ui| {
            ui.label(RichText::new("拉取镜像").weak());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.pull_spec)
                    .hint_text("例如 nginx:latest")
                    .desired_width(240.0),
            );
            let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can_pull = !self.pull_busy && !self.pull_spec.trim().is_empty();
            if icon_button(ui, ic::PLAY, "拉取镜像", ACCENT, can_pull)
                || (enter_pressed && can_pull)
            {
                need_pull = true;
            }
            if self.pull_busy {
                ui.spinner();
            }
        });

        // 批量操作条：有选中项才出现
        if selected_count > 0 {
            ui.horizontal(|ui| {
                ui.colored_label(BLUE, format!("已选 {selected_count} 项"));
                if icon_button(ui, ic::DELETE, "批量删除…", RED, true) {
                    batch_delete = true;
                }
                if icon_button(ui, ic::CLOSE, "清除选择", NEUTRAL, true) {
                    clear_sel = true;
                }
            });
        }
        ui.add_space(6.0);

        // 拉取进度（最后一行状态）
        if let Some(last) = self.pull_progress.last() {
            ui.label(RichText::new(last).weak().monospace());
        }

        if need_pull {
            self.start_pull();
        }
        if need_refresh {
            self.refresh_images();
        }
        if clear_sel {
            self.selected_images.clear();
        }

        // 搜索过滤：名称 / Tag / ID，不区分大小写
        let q = self.image_query.trim().to_lowercase();
        let mut rows: Vec<ImageInfo> = self
            .images
            .iter()
            .filter(|i| {
                if q.is_empty() {
                    return true;
                }
                i.name.to_lowercase().contains(&q)
                    || i.tag.to_lowercase().contains(&q)
                    || i.id.to_lowercase().contains(&q)
            })
            .cloned()
            .collect();

        // 排序：点表头切换；恒以 ID 兜底，保证刷新前后行序稳定不跳
        let (sort_col, ascending) = self.img_sort;
        rows.sort_by(|a, b| {
            let ord = cmp_image(a, b, sort_col);
            let ord = if ascending { ord } else { ord.reverse() };
            ord.then_with(|| a.id.cmp(&b.id))
        });

        // 批量删除：选中集合 ∩ 可见行
        if batch_delete {
            let items: Vec<(String, String)> = rows
                .iter()
                .filter(|i| self.selected_images.contains(&i.key()))
                .map(|i| (i.name.clone(), i.tag.clone()))
                .collect();
            if !items.is_empty() {
                self.confirm = Some(Confirm::DeleteImages { items });
            }
        }

        let all_selected =
            !rows.is_empty() && rows.iter().all(|i| self.selected_images.contains(&i.key()));
        let selected = self.selected_images.clone();
        let cols = self.img_cols.clone();
        let sort = self.img_sort;

        // 表格渲染完毕后统一应用，规避 egui 借用冲突
        let TableOutcome {
            select_all,
            sort_click,
            toggles,
            actions,
        } = image_table(ui, &rows, &selected, &cols, sort, all_selected);

        // 点表头：同列翻转方向，换列则按升序
        if let Some(col) = sort_click {
            if col == self.img_sort.0 {
                self.img_sort.1 = !self.img_sort.1;
            } else {
                self.img_sort = (col, true);
            }
        }

        if let Some(select) = select_all {
            for i in &rows {
                let key = i.key();
                if select {
                    self.selected_images.insert(key);
                } else {
                    self.selected_images.remove(&key);
                }
            }
        }
        for key in toggles {
            if !self.selected_images.insert(key.clone()) {
                self.selected_images.remove(&key);
            }
        }
        for action in actions {
            if let RowAction::DeleteImage(name, tag) = action {
                self.confirm = Some(Confirm::DeleteImage { name, tag });
            }
        }

        if rows.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    if !q.is_empty() {
                        ui.label(RichText::new("没有匹配的镜像").size(15.0).weak());
                        ui.label(RichText::new("换个关键词，或点搜索框右侧的 × 清空").weak());
                    } else if self.images.is_empty() {
                        ui.label(RichText::new("暂无镜像").size(15.0).weak());
                        ui.label(RichText::new("在上方输入镜像名，例如 nginx:latest").weak());
                    } else {
                        ui.label(RichText::new("没有符合条件的镜像").size(15.0).weak());
                        ui.label(RichText::new("换个关键词试试").weak());
                    }
                });
            });
        }
    }
}
