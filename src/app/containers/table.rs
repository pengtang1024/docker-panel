//! 容器表格渲染：表头排序/全选 + 行渲染调度（组头与容器行见 row 模块）

use super::row::render_row;
use super::*;

/// 渲染容器表格，返回需要上层应用的交互结果
#[allow(clippy::too_many_arguments)]
pub(super) fn container_table(
    ui: &mut egui::Ui,
    rows: &[ContainerInfo],
    disp: &[DispRow],
    selected: &HashSet<String>,
    cols: &[CtlCol],
    sort: (CtlCol, bool),
    all_selected: bool,
    collapsed_groups: &mut HashSet<String>,
) -> TableOutcome<CtlCol> {
    let min_width = 34.0 + cols.iter().map(|c| c.width()).sum::<f32>();
    let mut out = TableOutcome::default();

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
                body.rows(30.0, disp.len(), |row| {
                    render_row(
                        row,
                        disp,
                        rows,
                        selected,
                        cols,
                        collapsed_groups,
                        &mut out.toggles,
                        &mut out.actions,
                    );
                });
            });
    });

    out
}
