//! 容器表格行渲染：compose 组头（点击折叠）+ 容器单元格 + 右键菜单

use super::*;
use egui::Popup;

/// 渲染表格 body 里的一行：组头整行绘制，容器行按显示列逐格渲染
#[allow(clippy::too_many_arguments)]
pub(super) fn render_row(
    mut row: egui_extras::TableRow<'_, '_>,
    disp: &[DispRow],
    rows: &[ContainerInfo],
    selected: &HashSet<String>,
    cols: &[CtlCol],
    collapsed_groups: &mut HashSet<String>,
    toggles: &mut Vec<String>,
    actions: &mut Vec<RowAction>,
) {
    // ---- compose 组头：整行交给绘制层，点击折叠/展开 ----
    if let DispRow::Group { project, count } = &disp[row.index()] {
        let collapsed = collapsed_groups.contains(project);
        row.col(|_| {});
        for _ in 0..cols.len() {
            row.col(|_| {});
        }
        let resp = row.response();
        let painter = resp.ctx.layer_painter(resp.layer_id);
        let rect = resp.rect;
        let bg = if resp.hovered() { SURFACE_BG } else { PANEL_BG };
        painter.rect_filled(rect, 0.0, bg);
        // 左侧强调条，标识这是二级分组
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top()),
                egui::vec2(3.0, rect.height()),
            ),
            0.0,
            ACCENT,
        );
        fold_arrow(
            &painter,
            egui::pos2(rect.left() + 18.0, rect.center().y),
            !collapsed,
            TEXT_DIM,
        );
        let galley = painter.layout_no_wrap(
            format!("{project}  ·  {count} 个容器"),
            FontId::proportional(12.5),
            Color32::from_rgb(240, 243, 248),
        );
        painter.galley(
            egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y * 0.5),
            galley,
            Color32::from_rgb(240, 243, 248),
        );
        if resp.clicked() {
            if collapsed {
                collapsed_groups.remove(project);
            } else {
                collapsed_groups.insert(project.clone());
            }
        }
        return;
    }

    // ---- 普通容器行 ----
    let &DispRow::Container(i) = &disp[row.index()] else {
        return;
    };
    let c = &rows[i];
    let running = c.is_running();
    let id = c.id.clone();
    let name = c.name.clone();
    let checked = selected.contains(&id);
    // 选中底色：必须在第一个 col 之前设置才对整行生效
    row.set_selected(checked);

    // 行内任意控件被点时不再触发"整行切换选中"
    let mut widget_clicked = false;

    row.col(|ui| {
        if check_box(ui, checked) {
            widget_clicked = true;
            toggles.push(id.clone());
        }
    });
    for col in cols.iter().copied() {
        row.col(|ui| match col {
            CtlCol::Id => {
                ui.monospace(c.short_id());
            }
            CtlCol::Name => {
                ui.horizontal(|ui| {
                    ui.label(&c.name);
                    if let Some(project) = &c.compose_project {
                        let tip = match &c.compose_service {
                            Some(s) => format!("Compose 项目 {project} · 服务 {s}"),
                            None => format!("Compose 项目 {project}"),
                        };
                        chip(ui, "compose", CYAN, &tip);
                    }
                });
            }
            CtlCol::Image => {
                ui.label(&c.image);
            }
            CtlCol::State => {
                let (text, color) = state_label(&c.state);
                ui.colored_label(color, format!("● {text}"));
            }
            CtlCol::Uptime => {
                if c.status.is_empty() {
                    ui.weak("-");
                } else {
                    ui.label(&c.status);
                }
            }
            CtlCol::Ports => {
                if c.ports.is_empty() {
                    ui.weak("-");
                } else {
                    ui.label(&c.ports);
                }
            }
            CtlCol::Cpu => {
                if running {
                    let color = if c.cpu_percent >= 80.0 {
                        RED
                    } else {
                        ui.visuals().text_color()
                    };
                    right_label(
                        ui,
                        RichText::new(format!("{:.1}%", c.cpu_percent)).color(color),
                    );
                } else {
                    right_label(ui, RichText::new("-").weak());
                }
            }
            CtlCol::Mem => {
                if running && c.mem_limit > 0 {
                    right_label(
                        ui,
                        format!("{} ({:.1}%)", format_bytes(c.mem_usage), c.mem_percent()),
                    );
                } else if running {
                    right_label(ui, format_bytes(c.mem_usage));
                } else {
                    right_label(ui, RichText::new("-").weak());
                }
            }
            CtlCol::Disk => {
                ui.horizontal(|ui| {
                    right_label(ui, format_bytes(c.size_rw));
                    if c.size_root_fs > 0 {
                        right_label(
                            ui,
                            RichText::new(format!(" / {}", format_bytes(c.size_root_fs))).weak(),
                        );
                    }
                });
            }
            CtlCol::Net => {
                if running {
                    right_label(
                        ui,
                        format!(
                            "↓ {} / ↑ {}",
                            format_bytes(c.net_rx),
                            format_bytes(c.net_tx)
                        ),
                    );
                } else {
                    right_label(ui, RichText::new("-").weak());
                }
            }
            CtlCol::Actions => {
                // ⋯ 溢出按钮：压缩 6 个行内按钮为单一图标，点击弹出菜单
                let fg = NEUTRAL;
                let galley = ui.painter().layout_no_wrap(
                    ic::MORE.to_string(),
                    FontId::proportional(16.0),
                    fg,
                );
                let pad = egui::vec2(7.0, 5.0);
                let (rect, resp) =
                    ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::click());
                let bg = if resp.hovered() {
                    fg.gamma_multiply(0.30)
                } else {
                    fg.gamma_multiply(0.16)
                };
                ui.painter().rect_filled(rect, CornerRadius::same(5), bg);
                let pos = egui::pos2(rect.left() + pad.x, rect.center().y - galley.size().y * 0.5);
                ui.painter().galley(pos, galley, fg);
                let resp = resp.on_hover_text("更多操作");
                widget_clicked = resp.clicked();

                let mut action: Option<RowAction> = None;
                Popup::menu(&resp).show(|ui| {
                    render_row_menu(ui, &mut action, &id, &name, running);
                });
                if let Some(a) = action {
                    actions.push(a);
                }
            }
        });
    }

    let resp = row.response();

    // 右键菜单：操作就地完成，不必移到行尾
    let mut chosen: Option<RowAction> = None;
    resp.context_menu(|ui| {
        render_row_menu(ui, &mut chosen, &id, &name, running);
        ui.close();
    });
    if let Some(action) = chosen {
        actions.push(action);
    }

    // 双击打开日志；单击空白处切换选中
    if resp.double_clicked() {
        actions.push(RowAction::Logs(id.clone(), name.clone()));
    } else if resp.clicked() && !widget_clicked {
        toggles.push(id);
    }
}

/// 容器行右键菜单 & 行内 "⋯" 菜单的共享渲染
fn render_row_menu(
    ui: &mut egui::Ui,
    out: &mut Option<RowAction>,
    id: &str,
    name: &str,
    running: bool,
) {
    if menu_item(ui, ic::LOG, "查看日志", BLUE, true) {
        *out = Some(RowAction::Logs(id.to_string(), name.to_string()));
    }
    if menu_item(ui, ic::CHART, "资源监控", CYAN, true) {
        *out = Some(RowAction::Stats(id.to_string(), name.to_string()));
    }
    ui.separator();
    if menu_item(ui, ic::PLAY, "启动", GREEN, !running) {
        *out = Some(RowAction::Start(id.to_string(), name.to_string()));
    }
    if menu_item(ui, ic::STOP, "停止", RED, running) {
        *out = Some(RowAction::Stop(id.to_string(), name.to_string()));
    }
    if menu_item(ui, ic::RESTART, "重启", AMBER, running) {
        *out = Some(RowAction::Restart(id.to_string(), name.to_string()));
    }
    ui.separator();
    if menu_item(ui, ic::COPY, "复制容器 ID", NEUTRAL, true) {
        ui.ctx().copy_text(id.to_string());
    }
    if menu_item(ui, ic::DELETE, "删除容器", RED, true) {
        *out = Some(RowAction::DeleteContainer(id.to_string(), name.to_string()));
    }
}
