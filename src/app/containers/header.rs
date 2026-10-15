//! 顶部品牌头：标识 + Tab 切换 + Docker 连接状态 + 自定义窗口控制
//!
//! 无系统装饰窗口自绘：header 本身就是标题栏。
//! 惯用布局：单一 ui.horizontal（左→右）+ 中间弹性 space + 内嵌 right_to_left 推右。
//! 渲染完后 interact 整块 header 加 drag sense（后注册覆盖子控件），
//! 用户点 header 空白/品牌/Tab → 触发 StartDrag 移动窗口；点窗口按钮 → 响应 click。

use super::*;
use egui::viewport::ViewportCommand;

/// 窗口按钮尺寸
const BTN: egui::Vec2 = egui::vec2(26.0, 22.0);

impl DvApp {
    /// 顶部品牌头
    pub(in crate::app) fn ui_header(&mut self, ui: &mut egui::Ui) {
        ui.set_height(26.0);

        // 单一 horizontal：品牌 → 名称 → Tab → 弹性 space → (状态 + 窗口按钮)（内嵌 right_to_left）
        ui.horizontal(|ui| {
            // 品牌方块
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(18.0, 18.0),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(r);
            painter.rect_filled(r, CornerRadius::same(5), ACCENT);
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "D",
                FontId::proportional(12.0),
                Color32::WHITE,
            );

            ui.add_space(4.0);
            ui.label(
                RichText::new("dvkit")
                    .size(14.0)
                    .strong()
                    .color(Color32::from_rgb(240, 243, 248)),
            );

            ui.add_space(12.0);
            if nav_tab(ui, "容器", self.tab == Tab::Containers) {
                self.tab = Tab::Containers;
            }
            if nav_tab(ui, "镜像", self.tab == Tab::Images) {
                self.tab = Tab::Images;
            }

            // 弹性 space 把右侧内容推到窗口最右
            ui.add_space(ui.available_width());

            // 右侧：right_to_left 让窗口按钮从右往左排（最右是关闭 ✕）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.ui_window_controls(ui);

                let (dot, tip) = match self.docker_ok {
                    None => (AMBER, "正在连接 Docker…"),
                    Some(true) => (GREEN, "Docker 连接正常"),
                    Some(false) => (RED, "Docker 连接异常"),
                };
                ui.add_space(6.0);
                ui.colored_label(dot, RichText::new("●").size(10.5))
                    .on_hover_text(tip);
                ui.label(RichText::new(&self.status).weak());
            });
        });

        // ---- 给整块 header 区域加 drag sense ----
        // 后 interact → 覆盖之前 horizontal 里的子控件 sense
        // 这样拖 header 任何空白处/品牌/Tab → 触发 StartDrag
        // 窗口按钮在最右小区域，用户本能点按钮不会拖，click 正常响应
        let header_rect = ui.min_rect();
        let drag = ui.interact(
            header_rect,
            ui.id().with("header_drag"),
            egui::Sense::drag(),
        );

        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }

    /// 窗口控制按钮组（关闭 / 最大化还原 / 最小化）
    fn ui_window_controls(&self, ui: &mut egui::Ui) {
        let maximized = ui.ctx().input(|i| i.viewport().maximized).unwrap_or(false);

        // right_to_left 布局：代码顺序 = 视觉从右到左（最右是关闭按钮）
        let close_resp = close_x_button(ui);
        let max_resp = ctrl_icon_button(ui, if maximized { "❐" } else { "□" });
        let min_resp = ctrl_icon_button(ui, "—");

        let ctx = ui.ctx();
        if close_resp.clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if max_resp.clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }
        if min_resp.clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
    }
}

/// 关闭按钮：两条交叉线（避免字体不支持 "✕" 渲染成方块）
fn close_x_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(BTN, egui::Sense::click());
    let painter = ui.painter_at(rect);

    let pad = 6.5;
    let tl = rect.min + egui::vec2(pad, pad);
    let br = rect.max - egui::vec2(pad, pad);
    let tr = egui::pos2(br.x, tl.y);
    let bl = egui::pos2(tl.x, br.y);

    if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(207, 34, 46));
        painter.line_segment([tl, br], Stroke::new(1.4, Color32::WHITE));
        painter.line_segment([tr, bl], Stroke::new(1.4, Color32::WHITE));
    } else {
        let fg = Color32::from_rgb(201, 207, 214);
        painter.line_segment([tl, br], Stroke::new(1.4, fg));
        painter.line_segment([tr, bl], Stroke::new(1.4, fg));
    }
    resp
}

/// 通用图标按钮（最小化 / 最大化）
fn ctrl_icon_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(BTN, egui::Sense::click());
    let painter = ui.painter_at(rect);

    if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(55, 61, 71));
    }

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        if resp.hovered() {
            Color32::from_rgb(240, 243, 248)
        } else {
            Color32::from_rgb(201, 207, 214)
        },
    );
    resp
}
