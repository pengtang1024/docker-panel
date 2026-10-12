//! 交互控件：徽章、按钮、Tab、选择框、右键菜单项、搜索框

use egui::{pos2, Color32, CornerRadius, FontId, RichText, Stroke, TextStyle};

use crate::utils::{ACCENT, BLUE, SURFACE_BG, TEXT_DIM};

use super::{ic, NEUTRAL};

/// 批量删除确认弹窗中预览列表最多列出的条数，超出折叠为「… 等共 N 项」
const PREVIEW_MAX_ITEMS: usize = 8;

/// 彩色小徽章（如 compose 标识），悬停显示 tip
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32, tip: &str) {
    let font = FontId::new(10.5, egui::FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let pad = egui::vec2(5.0, 1.5);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), color.gamma_multiply(0.16));
    ui.painter().galley(
        pos2(rect.left() + pad.x, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
    let _ = resp.on_hover_text(tip);
}

/// 弹窗提示条：图标 + 同色文字，淡色圆角底
pub fn modal_note(ui: &mut egui::Ui, icon: char, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).size(15.0).color(color));
                ui.add(egui::Label::new(RichText::new(text).size(12.5).color(color)).wrap());
            });
        });
}

/// 弹窗底部按钮条：分隔线 + 右对齐按钮组
pub fn modal_footer(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.separator();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
}

/// 现代导航 Tab：悬停淡底 + 选中时文字高亮与底部强调横条
pub fn nav_tab(ui: &mut egui::Ui, text: &str, selected: bool) -> bool {
    let color = if selected {
        Color32::from_rgb(240, 243, 248)
    } else {
        TEXT_DIM
    };
    let font = FontId::new(13.5, egui::FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let pad = egui::vec2(14.0, 7.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::click());
    if resp.hovered() && !selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(16));
    }
    let pos = pos2(rect.left() + pad.x, rect.center().y - galley.size().y * 0.5);
    let text_width = galley.size().x;
    ui.painter().galley(pos, galley, color);
    if selected {
        // 底部强调横条
        let bar = egui::Rect::from_min_size(
            pos2(rect.left() + pad.x, rect.bottom() - 2.0),
            egui::vec2(text_width, 2.0),
        );
        ui.painter().rect_filled(bar, 1.0, ACCENT);
    }
    resp.clicked()
}

/// 行内扁平小按钮：语义色淡底 + 同色文字，无描边，悬停加深底色
pub fn ghost_button(ui: &mut egui::Ui, text: &str, color: Color32, enabled: bool) -> bool {
    let fg = if enabled { color } else { TEXT_DIM };
    // 沿用主题 Small 字号
    let font = ui
        .style()
        .text_styles
        .get(&TextStyle::Small)
        .cloned()
        .unwrap_or_else(|| FontId::new(11.5, egui::FontFamily::Proportional));
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, fg);
    let pad = egui::vec2(9.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::click());
    // 扁平底色：常态淡色，悬停加深；禁用置灰
    let bg = if enabled {
        let alpha = if resp.hovered() { 0.30 } else { 0.16 };
        color.gamma_multiply(alpha)
    } else {
        SURFACE_BG
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), bg);
    let pos = pos2(rect.left() + pad.x, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, fg);
    enabled && resp.clicked()
}

/// 扁平实底主按钮（弹窗/工具条主操作），无描边，禁用时置灰
pub fn solid_button(ui: &mut egui::Ui, text: &str, fill: Color32, enabled: bool) -> bool {
    let (fill, fg) = if enabled {
        (fill, Color32::WHITE)
    } else {
        (SURFACE_BG, TEXT_DIM)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(fg).strong())
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6)),
    )
    .clicked()
}

/// 批量删除确认的截断预览：最多列 PREVIEW_MAX_ITEMS 项，超出用 "… 等共 N 项" 收尾
pub fn preview_items(
    items: &[(String, String)],
    fmt: impl Fn(&(String, String)) -> String,
) -> String {
    let mut out = items
        .iter()
        .take(PREVIEW_MAX_ITEMS)
        .map(fmt)
        .collect::<Vec<_>>()
        .join("\n");
    if items.len() > PREVIEW_MAX_ITEMS {
        out.push_str(&format!("\n… 等共 {} 项", items.len()));
    }
    out
}

/// 图标按钮：Material 图标 + 悬浮提示（icon+tip），语义色淡底，返回是否被点击
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: char,
    tip: &str,
    color: Color32,
    enabled: bool,
) -> bool {
    let fg = if enabled { color } else { TEXT_DIM };
    // 该字体行高恰为 1em，16pt 图标 galley ≈ 16×16，可放进 30px 行高
    let galley = ui
        .painter()
        .layout_no_wrap(icon.to_string(), FontId::proportional(16.0), fg);
    let pad = egui::vec2(7.0, 5.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::click());
    let bg = if enabled {
        let alpha = if resp.hovered() { 0.30 } else { 0.16 };
        color.gamma_multiply(alpha)
    } else {
        SURFACE_BG
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), bg);
    let pos = pos2(rect.left() + pad.x, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, fg);
    // 禁用时也显示提示，让用户知道这个按钮是干什么的
    let resp = resp.on_hover_text(tip);
    enabled && resp.clicked()
}

/// 表格选择框：勾选态用 Material 图标绘制，返回是否被点击切换
pub fn check_box(ui: &mut egui::Ui, checked: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), Color32::from_white_alpha(16));
    }
    let color = if checked { BLUE } else { TEXT_DIM };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if checked { ic::CHECK_ON } else { ic::CHECK_OFF },
        FontId::proportional(15.0),
        color,
    );
    resp.clicked()
}

/// 右键菜单项：图标 + 文字，悬停淡底，返回是否被点击
pub fn menu_item(
    ui: &mut egui::Ui,
    icon: char,
    text: &str,
    icon_color: Color32,
    enabled: bool,
) -> bool {
    let fg = if enabled {
        ui.visuals().text_color()
    } else {
        TEXT_DIM
    };
    let icon_color = if enabled { icon_color } else { TEXT_DIM };
    let icon_galley =
        ui.painter()
            .layout_no_wrap(icon.to_string(), FontId::proportional(16.0), icon_color);
    let text_font = ui
        .style()
        .text_styles
        .get(&TextStyle::Small)
        .cloned()
        .unwrap_or_else(|| FontId::new(11.5, egui::FontFamily::Proportional));
    let text_galley = ui.painter().layout_no_wrap(text.to_owned(), text_font, fg);
    let width = (icon_galley.size().x + 10.0 + text_galley.size().x + 26.0).max(150.0);
    let height = (icon_galley.size().y.max(text_galley.size().y) + 8.0).max(24.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), Color32::from_white_alpha(16));
    }
    let cy = rect.center().y;
    let ix = rect.left() + 9.0;
    ui.painter().galley(
        pos2(ix, cy - icon_galley.size().y * 0.5),
        icon_galley,
        icon_color,
    );
    ui.painter().galley(
        pos2(ix + 16.0 + 10.0, cy - text_galley.size().y * 0.5),
        text_galley,
        fg,
    );
    enabled && resp.clicked()
}

/// 即时搜索框：放大镜图标 + 输入框 + 非空时出现一键清空按钮
pub fn search_box(ui: &mut egui::Ui, query: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(ic::SEARCH.to_string())
                .size(15.0)
                .color(TEXT_DIM),
        );
        ui.add(
            egui::TextEdit::singleline(query)
                .hint_text(hint)
                .desired_width(220.0),
        );
        if !query.is_empty() && icon_button(ui, ic::CLOSE, "清空搜索", NEUTRAL, true) {
            query.clear();
        }
    });
}
