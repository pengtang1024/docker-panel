//! 简易折线小图（CPU/内存资源曲线）

use egui::{pos2, Color32, CornerRadius, FontId, Pos2, Stroke};

/// 简易折线小图（values 为序列，超出 max 的按 max 截断）
pub fn draw_chart(ui: &mut egui::Ui, values: &[f64], max: f64, color: Color32) {
    let height = 110.0;
    let desired = egui::vec2(ui.available_width(), height);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // 背景与网格
    let bg = Color32::from_rgb(17, 21, 27);
    painter.rect_filled(rect, CornerRadius::same(6), bg);
    for i in 1..4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgb(38, 44, 52)),
        );
    }

    // 数据曲线
    if values.len() >= 2 && max > 0.0 {
        let n = values.len();
        let points: Vec<Pos2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
                let ratio = (v / max).clamp(0.0, 1.0) as f32;
                let y = rect.bottom() - ratio * rect.height();
                pos2(x, y)
            })
            .collect();

        // 面积填充：与背景预混色的不透明色，逐段矩形避免接缝/非凸填充问题
        let t = 0.18;
        let fill = Color32::from_rgb(
            (color.r() as f32 * t + bg.r() as f32 * (1.0 - t)).round() as u8,
            (color.g() as f32 * t + bg.g() as f32 * (1.0 - t)).round() as u8,
            (color.b() as f32 * t + bg.b() as f32 * (1.0 - t)).round() as u8,
        );
        for w in points.windows(2) {
            let area = egui::Rect::from_min_max(
                pos2(w[0].x, w[0].y.min(w[1].y)),
                pos2(w[1].x, rect.bottom()),
            );
            painter.rect_filled(area, 0.0, fill);
        }

        painter.add(egui::Shape::line(points, Stroke::new(1.8, color)));
    }

    // 当前值标注
    if let Some(last) = values.last() {
        painter.text(
            rect.right_top() - egui::vec2(8.0, -4.0),
            egui::Align2::RIGHT_TOP,
            format!("{last:.1}"),
            FontId::proportional(11.0),
            Color32::LIGHT_GRAY,
        );
    }
}
