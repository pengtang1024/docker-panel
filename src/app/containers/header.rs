//! 顶部品牌头：标识 + Tab 切换 + Docker 连接状态 + 自定义窗口控制
//!
//! Mutter/Wayland 无法跟随 GTK 深色主题，故关闭系统装饰自行绘制。
//! 拖拽用 StartDrag（egui-winit 在 Wayland 下翻译成 xdg_toplevel::move，
//! 由合成器处理窗口移动；X11 下同样有效）。
//!
//! 事件隔离：左侧品牌+Tab 区域注册 drag，右侧按钮+状态区域不注册 drag，
//! 这样按钮的 click 事件不会被 drag 抢占。

use super::*;
use egui::viewport::ViewportCommand;

impl DvApp {
    /// 顶部品牌头：标识 + Tab + 连接状态 + 自定义窗口控制
    ///
    /// 布局：单一 ui.horizontal 左到右 → 弹性 space → 内嵌 right_to_left 推到最右。
    /// 渲染完后 interact 加 drag sense（后注册覆盖子控件）。
    pub(in crate::app) fn ui_header(&mut self, ui: &mut egui::Ui) {
        ui.set_height(26.0);

        ui.horizontal(|ui| {
            // 品牌方块
            let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            let painter = ui.painter_at(r);
            painter.rect_filled(r, CornerRadius::same(5), ACCENT);
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "D",
                FontId::proportional(12.0),
                Color32::WHITE,
            );
            ui.label(
                RichText::new("Docker Panel")
                    .size(14.0)
                    .strong()
                    .color(Color32::from_rgb(240, 243, 248)),
            );

            ui.add_space(14.0);
            if nav_tab(ui, "容器", self.tab == Tab::Containers) {
                self.tab = Tab::Containers;
            }
            if nav_tab(ui, "镜像", self.tab == Tab::Images) {
                self.tab = Tab::Images;
            }

            // 弹性 space 把右侧推到最右
            ui.add_space(ui.available_width());

            // 右侧（right_to_left：最右关闭按钮 → 依次往左是状态文本）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.ui_window_controls(ui);
                let (dot, tip) = match self.docker_ok {
                    None => (AMBER, "正在连接 Docker…"),
                    Some(true) => (GREEN, "Docker 连接正常"),
                    Some(false) => (RED, "Docker 连接异常"),
                };
                ui.add_space(8.0);
                ui.colored_label(dot, RichText::new("●").size(10.5))
                    .on_hover_text(tip);
                ui.label(RichText::new(&self.status).weak());
            });
        });

        // 整块 header rect（Panel::top frame 内的）
        let header_rect = ui.min_rect();

        // drag sense 后注册 → 覆盖子控件 click
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

        // right_to_left 布局：代码顺序 = 视觉从右到左
        let close_resp = close_x_button(ui); // 最右：✕
        let max_resp = ctrl_icon_button(ui, if maximized { "❐" } else { "□" });
        let min_resp = ctrl_icon_button(ui, "—"); // 最左：—

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
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::click());
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
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::click());
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
