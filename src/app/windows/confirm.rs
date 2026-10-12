//! 二次确认弹窗（删除容器 / 删除镜像，含批量预览）

use super::*;

impl DvApp {
    /// 二次确认弹窗
    pub(in crate::app) fn ui_confirm_window(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.confirm.clone() else {
            return;
        };
        let (title, question, warn, preview): (&str, String, &str, Option<Vec<String>>) =
            match &confirm {
                Confirm::DeleteContainer { name, .. } => (
                    "删除容器",
                    format!("确定要删除容器 {name} 吗？"),
                    "运行中的容器会被强制删除，此操作不可恢复。",
                    None,
                ),
                Confirm::DeleteImage { name, tag } => (
                    "删除镜像",
                    format!("确定要删除镜像 {name}:{tag} 吗？"),
                    "被容器引用的镜像无法删除，此操作不可恢复。",
                    None,
                ),
                Confirm::DeleteContainers { items } => (
                    "批量删除容器",
                    format!("确定要删除这 {} 个容器吗？", items.len()),
                    "运行中的容器会被强制删除，此操作不可恢复。",
                    Some(
                        preview_items(items, |(id, name)| {
                            format!("{name} ({})", &id[..id.len().min(12)])
                        })
                        .lines()
                        .map(str::to_owned)
                        .collect(),
                    ),
                ),
                Confirm::DeleteImages { items } => (
                    "批量删除镜像",
                    format!("确定要删除这 {} 个镜像吗？", items.len()),
                    "被容器引用的镜像无法删除，此操作不可恢复。",
                    Some(
                        preview_items(items, |(name, tag)| format!("{name}:{tag}"))
                            .lines()
                            .map(str::to_owned)
                            .collect(),
                    ),
                ),
            };
        let mut open = true;
        let mut accepted = false;
        egui::Window::new(title)
            .id(egui::Id::new("dvkit_confirm_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(question).size(13.5));
                ui.add_space(8.0);
                modal_note(ui, ic::WARNING, warn, RED);
                if let Some(items) = &preview {
                    ui.add_space(8.0);
                    mono_box(
                        ui,
                        &items.iter().map(String::as_str).collect::<Vec<_>>(),
                        None,
                        170.0,
                    );
                }
                modal_footer(ui, |ui| {
                    if solid_button(ui, "确认删除", RED, true) {
                        accepted = true;
                        open = false;
                    }
                    if ghost_button(ui, "取消", NEUTRAL, true) {
                        open = false;
                    }
                });
            });
        if !open {
            self.confirm = None;
            if accepted {
                match confirm {
                    Confirm::DeleteContainer { id, name } => {
                        self.spawn_action(ActionKind::Remove, id, name)
                    }
                    Confirm::DeleteImage { name, tag } => {
                        let tx = self.tx.clone();
                        let desc = format!("删除 {name}:{tag}");
                        self.rt.spawn(async move {
                            let r = docker::remove_image(&name, &tag).await;
                            let _ = tx.send(Msg::ActionDone(desc, r));
                        });
                    }
                    Confirm::DeleteContainers { items } => {
                        self.spawn_batch_action(ActionKind::Remove, items);
                        self.selected_containers.clear();
                    }
                    Confirm::DeleteImages { items } => {
                        self.spawn_batch_delete_images(items);
                        self.selected_images.clear();
                    }
                }
            }
        }
    }
}
