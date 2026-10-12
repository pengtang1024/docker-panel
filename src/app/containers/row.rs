//! 容器表格行渲染：compose 组头（点击折叠）+ 容器单元格 + 右键菜单

use super::*;

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
                if running {
                    right_label(
                        ui,
                        format!(
                            "R {} / W {}",
                            format_bytes(c.block_read),
                            format_bytes(c.block_write)
                        ),
                    );
                } else {
                    right_label(ui, RichText::new("-").weak());
                }
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
                ui.horizontal(|ui| {
                    if icon_button(ui, ic::LOG, "查看日志", BLUE, true) {
                        widget_clicked = true;
                        actions.push(RowAction::Logs(id.clone(), name.clone()));
                    }
                    if icon_button(ui, ic::CHART, "资源监控", CYAN, true) {
                        widget_clicked = true;
                        actions.push(RowAction::Stats(id.clone(), name.clone()));
                    }
                    if icon_button(ui, ic::PLAY, "启动", GREEN, !running) {
                        widget_clicked = true;
                        actions.push(RowAction::Start(id.clone(), name.clone()));
                    }
                    if icon_button(ui, ic::STOP, "停止", RED, running) {
                        widget_clicked = true;
                        actions.push(RowAction::Stop(id.clone(), name.clone()));
                    }
                    if icon_button(ui, ic::RESTART, "重启", AMBER, running) {
                        widget_clicked = true;
                        actions.push(RowAction::Restart(id.clone(), name.clone()));
                    }
                    if icon_button(ui, ic::DELETE, "删除容器", RED, true) {
                        widget_clicked = true;
                        actions.push(RowAction::DeleteContainer(id.clone(), name.clone()));
                    }
                });
            }
        });
    }

    let resp = row.response();

    // 右键菜单：操作就地完成，不必移到行尾
    let mut chosen: Option<RowAction> = None;
    resp.context_menu(|ui| {
        if menu_item(ui, ic::LOG, "查看日志", BLUE, true) {
            chosen = Some(RowAction::Logs(id.clone(), name.clone()));
        }
        if menu_item(ui, ic::CHART, "资源监控", CYAN, true) {
            chosen = Some(RowAction::Stats(id.clone(), name.clone()));
        }
        ui.separator();
        if menu_item(ui, ic::PLAY, "启动", GREEN, !running) {
            chosen = Some(RowAction::Start(id.clone(), name.clone()));
        }
        if menu_item(ui, ic::STOP, "停止", RED, running) {
            chosen = Some(RowAction::Stop(id.clone(), name.clone()));
        }
        if menu_item(ui, ic::RESTART, "重启", AMBER, running) {
            chosen = Some(RowAction::Restart(id.clone(), name.clone()));
        }
        ui.separator();
        if menu_item(ui, ic::COPY, "复制容器 ID", NEUTRAL, true) {
            ui.ctx().copy_text(id.clone());
        }
        if menu_item(ui, ic::DELETE, "删除容器", RED, true) {
            chosen = Some(RowAction::DeleteContainer(id.clone(), name.clone()));
        }
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
