//! 镜像表格渲染：表头排序/全选 + 行单元格 + 右键菜单
use super::*;
use crate::widgets::CELL_PAD;
use crate::widgets::TABLE_PAD;
/// 渲染镜像表格（横向滚动 + 可拖宽列 + 排序表头）
pub(super) fn image_table(
    ui: &mut egui::Ui,
    rows: &[ImageInfo],
    selected: &HashSet<String>,
    cols: &[ImgCol],
    sort: (ImgCol, bool),
    all_selected: bool,
    multi_select: bool,
) -> TableOutcome<ImgCol> {
    let mut out = TableOutcome::default();
    // checkbox 列只在多选模式下存在，宽度也要跟着算
    let check_w = if multi_select { CHECK_COL_W } else { 0.0 };
    let min_width = check_w + cols.iter().map(|c| c.width()).sum::<f32>();
    // 必须用 both() 而非 horizontal()：单方向 ScrollArea 会把纵向滚轮
    // 映射到自身方向（always_scroll_the_only_direction），行数多时
    // 滚轮会被横向吃掉，表头无法随内容滚走。
    // 表格四周留边距：egui_extras 没有单元格内边距 API，
    // 用外层 Frame 统一留出 TABLE_PAD，首列才不会贴窗口边缘。
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(TABLE_PAD, 0))
        .show(ui, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.set_min_width(min_width);
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    // 整行可点击：hover 高亮 + 右键菜单
                    .sense(egui::Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    // 纵向已由外层 ScrollArea 负责，表格自身不再嵌套一层
                    .vscroll(false);
                if multi_select {
                    // 选择列固定宽度，不参与拖拽
                    table = table.column(
                        Column::initial(CHECK_COL_W)
                            .at_least(CHECK_COL_W)
                            .resizable(false),
                    );
                }
                for (i, col) in cols.iter().enumerate() {
                    table = table.column(col.column(i + 1 == cols.len()));
                }
                table
                    .header(30.0, |mut header| {
                        // 表头全选框
                        if multi_select {
                            header.col(|ui| {
                                let rect = ui.max_rect().expand2(egui::vec2(2.0, 0.0));
                                ui.painter().rect_filled(rect, 0.0, SURFACE_BG);
                                if check_box(ui, all_selected) {
                                    out.select_all = Some(!all_selected);
                                }
                            });
                        }
                        for col in cols.iter().copied() {
                            header.col(|ui| {
                                cell(ui, |ui| {
                                    if col.sortable() {
                                        if sort_header(
                                            ui,
                                            col.title(),
                                            sort.0 == col,
                                            sort.1,
                                            col.right(),
                                        ) {
                                            out.sort_click = Some(col);
                                        }
                                    } else {
                                        header_cell(ui, col.title());
                                    }
                                });
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(30.0, rows.len(), |mut row| {
                            let img = &rows[row.index()];
                            let key = img.key();
                            let checked = multi_select && selected.contains(&key);
                            // 选中底色：必须在第一个 col 之前设置才对整行生效
                            row.set_selected(checked);
                            let mut widget_clicked = false;
                            if multi_select {
                                row.col(|ui| {
                                    if check_box(ui, checked) {
                                        widget_clicked = true;
                                        out.toggles.push(key.clone());
                                    }
                                });
                            }
                            for col in cols.iter().copied() {
                                // CELL_PAD 只推进光标、不分配控件，cell 的 used_rect
                                // 因此恒为「CELL_PAD + 内容自然宽」，与列宽无关
                                row.col(|ui| {
                                    ui.add_space(CELL_PAD);
                                    match col {
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
                                            // 主按钮只留删除，其余进二级菜单（此处镜像可做的事还很少）
                                            ui.vertical_centered(|ui| {
                                                ui.horizontal(|ui| {
                                                    if icon_button(
                                                        ui,
                                                        ic::DELETE,
                                                        "删除镜像",
                                                        RED,
                                                        true,
                                                    ) {
                                                        widget_clicked = true;
                                                        out.actions.push(RowAction::DeleteImage(
                                                            img.name.clone(),
                                                            img.tag.clone(),
                                                        ));
                                                    }
                                                });
                                            });
                                        }
                                    }
                                });
                            } // for col
                            let resp = row.response();
                            // 右键菜单：删除 / 复制镜像 ID 就地完成
                            let mut chosen: Option<RowAction> = None;
                            resp.context_menu(|ui| {
                                if menu_item(ui, ic::COPY, "复制镜像 ID", NEUTRAL, true) {
                                    ui.ctx().copy_text(img.id.clone());
                                }
                                ui.separator();
                                if menu_item(ui, ic::DELETE, "删除镜像", RED, true) {
                                    chosen = Some(RowAction::DeleteImage(
                                        img.name.clone(),
                                        img.tag.clone(),
                                    ));
                                }
                                ui.close();
                            });
                            if let Some(action) = chosen {
                                out.actions.push(action);
                            }
                            // 单击切换选中仅在多选模式下生效（默认态单击不产生选中）
                            if multi_select && resp.clicked() && !widget_clicked {
                                out.toggles.push(key);
                            }
                        });
                    });
            });
        });
    out
}