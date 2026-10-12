//! 表头组件：单元格底色、排序表头、手绘三角箭头

use egui::{pos2, Color32, FontId, Pos2, RichText, Stroke};

use crate::utils::SURFACE_BG;

/// 表头文字样式（所有表头统一字号与颜色）
const HEADER_FONT: FontId = FontId::new(12.5, egui::FontFamily::Proportional);
/// 表头文字颜色
const HEADER_FG: Color32 = Color32::from_rgb(201, 207, 214);

pub fn header_cell(ui: &mut egui::Ui, name: &str) {
    // 向左右各扩 2px 盖住单元格间隙
    let rect = ui.max_rect().expand2(egui::vec2(2.0, 0.0));
    ui.painter().rect_filled(rect, 0.0, SURFACE_BG);
    ui.label(
        RichText::new(name)
            .font(HEADER_FONT.clone())
            .strong()
            .color(HEADER_FG),
    );
}

/// 可点击排序的表头：返回是否被点击（active 表示当前列正在排序，ascending 为方向）
pub fn sort_header(
    ui: &mut egui::Ui,
    title: &str,
    active: bool,
    ascending: bool,
    right: bool,
) -> bool {
    // 先占满整个表头单元格，保证整格都可点击
    let size = ui.max_rect().size();
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    ui.painter()
        .rect_filled(rect.expand2(egui::vec2(2.0, 0.0)), 0.0, SURFACE_BG);
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_white_alpha(10));
    }

    let color = if active {
        Color32::from_rgb(240, 243, 248)
    } else {
        HEADER_FG
    };
    let galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), HEADER_FONT.clone(), color);
    let cy = rect.center().y;
    let arrow_w = if active { 14.0 } else { 6.0 };
    let text_x = if right {
        rect.right() - 8.0 - arrow_w - galley.size().x
    } else {
        rect.left() + 8.0
    };
    let arrow_x = if right {
        rect.right() - 10.0
    } else {
        text_x + galley.size().x + 10.0
    };
    ui.painter()
        .galley(pos2(text_x, cy - galley.size().y * 0.5), galley, color);
    if active {
        sort_arrow(ui.painter(), pos2(arrow_x, cy), ascending, color);
    }
    resp.clicked()
}

/// 手绘排序三角（▲/▼ 在部分字体里缺字，直接画多边形最稳）
pub fn sort_arrow(painter: &egui::Painter, origin: Pos2, up: bool, color: Color32) {
    let (dx, dy) = (4.0f32, 3.5f32);
    let pts = if up {
        vec![
            pos2(origin.x, origin.y - dy),
            pos2(origin.x + dx, origin.y + dy),
            pos2(origin.x - dx, origin.y + dy),
        ]
    } else {
        vec![
            pos2(origin.x, origin.y + dy),
            pos2(origin.x + dx, origin.y - dy),
            pos2(origin.x - dx, origin.y - dy),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// 手绘分组折叠三角：expanded 朝下，折叠时朝右
pub fn fold_arrow(painter: &egui::Painter, origin: Pos2, expanded: bool, color: Color32) {
    let (dx, dy) = (4.5f32, 4.0f32);
    let pts = if expanded {
        vec![
            pos2(origin.x - dx, origin.y - dy),
            pos2(origin.x + dx, origin.y - dy),
            pos2(origin.x, origin.y + dy),
        ]
    } else {
        vec![
            pos2(origin.x - dy, origin.y - dx),
            pos2(origin.x - dy, origin.y + dx),
            pos2(origin.x + dy, origin.y),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// 右对齐单元格文本（数值列用）
pub fn right_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}
