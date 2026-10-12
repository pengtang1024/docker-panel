//! 顶部品牌头：标识 + Tab 切换 + Docker 连接状态

use super::*;

impl DvApp {
    /// 顶部品牌头：标识 + Tab + 连接状态
    pub(in crate::app) fn ui_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // 品牌标识：蓝色圆角方块 + 产品名
            let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, CornerRadius::same(6), ACCENT);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "D",
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            ui.label(
                RichText::new("dvkit")
                    .size(16.0)
                    .strong()
                    .color(Color32::from_rgb(240, 243, 248)),
            );

            ui.add_space(18.0);
            if nav_tab(ui, "容器", self.tab == Tab::Containers) {
                self.tab = Tab::Containers;
            }
            if nav_tab(ui, "镜像", self.tab == Tab::Images) {
                self.tab = Tab::Images;
            }

            // 右侧：连接状态点 + 状态栏文本
            // 角标只跟"连接是否正常"绑定，不再随刷新进度变色
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (dot, tip) = match self.docker_ok {
                    None => (AMBER, "正在连接 Docker…"),
                    Some(true) => (GREEN, "Docker 连接正常"),
                    Some(false) => (RED, "Docker 连接异常"),
                };
                ui.colored_label(dot, RichText::new("●").size(11.0))
                    .on_hover_text(tip);
                ui.label(RichText::new(&self.status).weak());
            });
        });
    }
}
