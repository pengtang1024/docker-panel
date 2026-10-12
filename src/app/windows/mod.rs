//! 各类弹窗：日志、资源监控、清理、二次确认、错误、执行结果
//! 统一居中弹出、统一提示条与底部按钮条，尺寸按内容重新标定

mod confirm;
mod dialogs;

use super::*;
use crate::widgets::*;

impl DvApp {
    /// 日志窗口
    pub(super) fn ui_log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.log_view.is_some();
        if let Some(lv) = self.log_view.as_mut() {
            egui::Window::new(format!("日志 — {}", lv.name))
                .id(egui::Id::new(WIN_LOG))
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(900.0)
                .default_height(560.0)
                .min_width(560.0)
                .min_height(320.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        // 暂停/继续滚动：同位置同尺寸，icon 表达当前可执行的动作
                        let (icon, tip) = if lv.follow {
                            (ic::PAUSE, "暂停滚动")
                        } else {
                            (ic::PLAY, "继续滚动")
                        };
                        let follow_color = if lv.follow { AMBER } else { GREEN };
                        if icon_button(ui, icon, tip, follow_color, true) {
                            lv.follow = !lv.follow;
                        }
                        if icon_button(ui, ic::DELETE, "清空日志", RED, true) {
                            lv.lines.clear();
                        }
                        ui.separator();
                        if lv.ended {
                            chip(ui, "已结束", TEXT_DIM, "日志流已结束");
                        } else {
                            ui.spinner();
                            chip(ui, "实时", GREEN, "日志流实时接收中");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.monospace(format!("{} 行", lv.lines.len()));
                            // 容器短 ID：多个日志窗并排时便于区分
                            ui.weak(lv.id.chars().take(12).collect::<String>());
                        });
                    });
                    ui.separator();
                    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                    // 深色终端风底色
                    egui::Frame::new()
                        .fill(DEEP_BG)
                        .inner_margin(egui::Margin::same(8))
                        .corner_radius(CornerRadius::same(6))
                        .stroke(Stroke::new(1.0, DIVIDER))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .stick_to_bottom(lv.follow)
                                .show_rows(ui, row_height, lv.lines.len(), |ui, range| {
                                    for line in &lv.lines[range] {
                                        ui.monospace(line);
                                    }
                                });
                        });
                });
        }
        if !open {
            self.close_log();
        }
    }

    /// 资源监控窗口（指标卡 + CPU / 内存折线小图）
    pub(super) fn ui_resource_window(&mut self, ctx: &egui::Context) {
        let mut open = self.resource_view.is_some();
        if let Some(rv) = self.resource_view.as_mut() {
            let latest = rv.history.back().copied();
            egui::Window::new(format!("资源监控 — {}", rv.name))
                .id(egui::Id::new(WIN_RESOURCE))
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(560.0)
                .min_width(460.0)
                .collapsible(false)
                .show(ctx, |ui| {
                    if let Some(p) = latest {
                        let mem_pct = if p.mem_limit > 0 {
                            p.mem_usage as f64 / p.mem_limit as f64 * 100.0
                        } else {
                            0.0
                        };
                        ui.columns(2, |cols| {
                            metric_card(
                                &mut cols[0],
                                "CPU",
                                format!("{:.1}%", p.cpu),
                                "当前使用率".to_owned(),
                                BLUE,
                            );
                            metric_card(
                                &mut cols[1],
                                "内存",
                                format_bytes(p.mem_usage),
                                format!("/ {} · {:.1}%", format_bytes(p.mem_limit), mem_pct),
                                GREEN,
                            );
                        });
                    } else {
                        ui.weak("采样中…（仅运行中的容器有数据）");
                    }

                    // CPU 曲线
                    let cpu_values: Vec<f64> = rv.history.iter().map(|p| p.cpu).collect();
                    let cpu_max = cpu_values.iter().cloned().fold(10.0f64, f64::max) * 1.2;
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("CPU 使用率 (%)")
                                .size(12.0)
                                .strong()
                                .color(Color32::from_rgb(201, 207, 214)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(format!("{cpu_max:.0} 上限"))
                        });
                    });
                    draw_chart(ui, &cpu_values, cpu_max, BLUE);

                    // 内存曲线（按上限百分比）
                    let mem_values: Vec<f64> = rv
                        .history
                        .iter()
                        .map(|p| {
                            if p.mem_limit > 0 {
                                p.mem_usage as f64 / p.mem_limit as f64 * 100.0
                            } else {
                                0.0
                            }
                        })
                        .collect();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("内存占用 (%)")
                            .size(12.0)
                            .strong()
                            .color(Color32::from_rgb(201, 207, 214)),
                    );
                    draw_chart(ui, &mem_values, 100.0, GREEN);
                });
        }
        if !open {
            self.resource_view = None;
        }
    }
}

/// 弹窗里的指标卡：小标题 + 大号数值 + 次要说明
fn metric_card(ui: &mut egui::Ui, title: &str, value: String, hint: String, color: Color32) {
    egui::Frame::new()
        .fill(SURFACE_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.colored_label(color, RichText::new(title).size(11.5));
            ui.label(
                RichText::new(value)
                    .size(19.0)
                    .strong()
                    .color(Color32::from_rgb(240, 243, 248)),
            );
            ui.weak(hint);
        });
}

/// 等宽深底文本框：错误详情 / 结果 / 删除清单
fn mono_box(ui: &mut egui::Ui, lines: &[&str], color: Option<Color32>, max_height: f32) {
    egui::Frame::new()
        .fill(DEEP_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(max_height)
                .show(ui, |ui| {
                    for line in lines {
                        let text = RichText::new(*line).monospace();
                        let text = match color {
                            Some(c) => text.color(c),
                            None => text,
                        };
                        ui.add(egui::Label::new(text).wrap());
                    }
                });
        });
}

/// 清理选项行：勾选框 + 标题 + 说明，勾选后底色带主题色
fn opt_row(ui: &mut egui::Ui, checked: &mut bool, title: &str, hint: &str) {
    let bg = if *checked {
        ACCENT.gamma_multiply(0.14)
    } else {
        SURFACE_BG
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if check_box(ui, *checked) {
                    *checked = !*checked;
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(13.0));
                    ui.weak(hint);
                });
            });
        });
}
