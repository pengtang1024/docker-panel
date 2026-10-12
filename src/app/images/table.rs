//! 镜像表格渲染：表头排序/全选 + 行单元格 + 右键菜单

use super::*;

/// 渲染镜像表格（横向滚动 + 可拖宽列 + 排序表头）
pub(super) fn image_table(
    ui: &mut egui::Ui,
    rows: &[ImageInfo],
    selected: &HashSet<String>,
    cols: &[ImgCol],
    sort: (ImgCol, bool),
    all_selected: bool,
) -> TableOutcome<ImgCol> {
    let mut out = TableOutcome::default();
    let min_width = 34.0 + cols.iter().map(|c| c.width()).sum::<f32>();

    ScrollArea::horizontal().show(ui, |ui| {
        ui.set_min_width(min_width);
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click()) // 整行 hover 高亮 + 右键菜单
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .vscroll(false)
            .column(Column::initial(34.0).at_least(34.0).resizable(false)); // 选择
        for (i, col) in cols.iter().enumerate() {
            table = table.column(col.column(i + 1 == cols.len()));
        }
        table
            .header(30.0, |mut header| {
                // 表头全选框
                header.col(|ui| {
                    let rect = ui.max_rect().expand2(egui::vec2(2.0, 0.0));
                    ui.painter().rect_filled(rect, 0.0, SURFACE_BG);
                    if check_box(ui, all_selected) {
                        out.select_all = Some(!all_selected);
                    }
                });
                for col in cols.iter().copied() {
                    header.col(|ui| {
                        if col.sortable() {
                            if sort_header(ui, col.title(), sort.0 == col, sort.1, col.right()) {
                                out.sort_click = Some(col);
                            }
                        } else {
                            header_cell(ui, col.title());
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, rows.len(), |mut row| {
                    let img = &rows[row.index()];
                    let key = img.key();
                    let checked = selected.contains(&key);
                    // 选中底色：必须在第一个 col 之前设置才对整行生效
                    row.set_selected(checked);

                    let mut widget_clicked = false;

                    row.col(|ui| {
                        if check_box(ui, checked) {
                            widget_clicked = true;
                            out.toggles.push(key.clone());
                        }
                    });
                    for col in cols.iter().copied() {
                        row.col(|ui| match col {
                            ImgCol::Name => {
                                let tip = format!("{} · {}", img.short_id(), img.id);
                                let text = if img.dangling {
                                    egui::RichText::new(&img.name).color(AMBER)
                                } else {
                                    egui::RichText::new(&img.name)
                                };
                                ui.label(text).on_hover_text(tip);
                            }
                            ImgCol::Tag => {
                                ui.monospace(&img.tag);
                            }
                            ImgCol::Size => {
                                right_label(ui, format_bytes(img.size as u64));
                            }
                            ImgCol::Created => {
                                ui.label(format_age(img.created));
                            }
                            ImgCol::Refs => {
                                right_label(ui, format!("{}", img.containers));
                            }
                            ImgCol::Actions => {
                                if icon_button(ui, ic::DELETE, "删除镜像", RED, true) {
                                    widget_clicked = true;
                                    out.actions.push(RowAction::DeleteImage(
                                        img.name.clone(),
                                        img.tag.clone(),
                                    ));
                                }
                            }
                        });
                    }

                    let resp = row.response();

                    // 右键菜单：删除 / 复制镜像 ID 就地完成
                    let mut chosen: Option<RowAction> = None;
                    resp.context_menu(|ui| {
                        if menu_item(ui, ic::COPY, "复制镜像 ID", NEUTRAL, true) {
                            ui.ctx().copy_text(img.id.clone());
                        }
                        ui.separator();
                        if menu_item(ui, ic::DELETE, "删除镜像", RED, true) {
                            chosen =
                                Some(RowAction::DeleteImage(img.name.clone(), img.tag.clone()));
                        }
                        ui.close();
                    });
                    if let Some(action) = chosen {
                        out.actions.push(action);
                    }

                    // 单击空白处切换选中
                    if resp.clicked() && !widget_clicked {
                        out.toggles.push(key);
                    }
                });
            });
    });

    out
}
