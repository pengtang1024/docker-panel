//! 清理 / 错误 / 执行结果三个提示类弹窗

use super::*;

impl DvApp {
    /// 清理弹窗（勾选 + 执行前确认）
    pub(in crate::app) fn ui_cleanup_window(&mut self, ctx: &egui::Context) {
        if !self.cleanup_open {
            return;
        }
        let mut open = true;
        let mut execute = false;
        let mut decided: Option<bool> = None;
        egui::Window::new("简易清理")
            .id(egui::Id::new("dvkit_cleanup_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(440.0)
            .open(&mut open)
            .show(ctx, |ui| {
                modal_note(
                    ui,
                    ic::WARNING,
                    "清理后内容无法恢复，请确认勾选范围。",
                    AMBER,
                );
                ui.add_space(8.0);
                opt_row(
                    ui,
                    &mut self.cleanup_stopped,
                    "已停止的容器",
                    "状态为 exited / created 的容器",
                );
                ui.add_space(4.0);
                opt_row(
                    ui,
                    &mut self.cleanup_dangling,
                    "悬空镜像",
                    "没有 Tag、也没被其他镜像引用的镜像",
                );
                ui.add_space(4.0);
                opt_row(
                    ui,
                    &mut self.cleanup_unused,
                    "未使用镜像",
                    "所有未被任何容器使用的镜像（含 Tag）",
                );
                modal_footer(ui, |ui| {
                    let any = self.cleanup_stopped || self.cleanup_dangling || self.cleanup_unused;
                    if solid_button(ui, "确认执行", ACCENT, any) {
                        decided = Some(true);
                    }
                    if ghost_button(ui, "取消", NEUTRAL, true) {
                        decided = Some(false);
                    }
                });
            });
        if let Some(accept) = decided {
            open = false;
            execute = accept;
        }
        self.cleanup_open = open;
        if execute {
            let (stopped, dangling, unused) = (
                self.cleanup_stopped,
                self.cleanup_dangling,
                self.cleanup_unused,
            );
            let tx = self.tx.clone();
            self.rt.spawn(async move {
                let msg = Msg::CleanupDone(docker::cleanup(stopped, dangling, unused).await);
                let _ = tx.send(msg);
            });
        }
    }

    /// 错误弹窗（Docker API 调用失败）
    pub(in crate::app) fn ui_error_window(&mut self, ctx: &egui::Context) {
        let Some(err) = self.error.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("错误详情")
            .id(egui::Id::new("dvkit_error_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(500.0)
            .max_width(640.0)
            .show(ctx, |ui| {
                modal_note(ui, ic::ERROR, "Docker 调用失败，本次操作没有执行。", RED);
                ui.add_space(8.0);
                // 批量操作的错误是多行聚合文本，等宽逐行滚动展示
                mono_box(ui, &err.lines().collect::<Vec<_>>(), Some(RED), 240.0);
                modal_footer(ui, |ui| {
                    if solid_button(ui, "确定", ACCENT, true) {
                        open = false;
                    }
                });
            });
        if !open {
            self.error = None;
            self.last_error.clear();
        }
    }

    /// 信息弹窗（操作/清理结果）
    pub(in crate::app) fn ui_result_window(&mut self, ctx: &egui::Context) {
        let Some(text) = self.result.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("执行结果")
            .id(egui::Id::new("dvkit_result_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(460.0)
            .show(ctx, |ui| {
                modal_note(ui, ic::SUCCESS, "操作已完成", GREEN);
                ui.add_space(8.0);
                mono_box(ui, &text.lines().collect::<Vec<_>>(), None, 240.0);
                modal_footer(ui, |ui| {
                    if solid_button(ui, "确定", ACCENT, true) {
                        open = false;
                    }
                });
            });
        if !open {
            self.result = None;
        }
    }
}
