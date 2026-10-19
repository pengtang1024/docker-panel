//! 底部状态栏：左侧最近一次操作结果，右侧 Docker 概况摘要
//!
//! 与顶部品牌头分工：顶栏只管品牌、Tab 切换与 Docker 连接角标，
//! 状态文本与统计下沉到这里，让顶栏保持清爽。
//!
//! 摘要常驻可见；完整概况（版本、驱动、CPU/内存、容器镜像计数）
//! 由 `ui_docker_info` 在弹窗区渲染——egui 窗口必须与 Panel 同级，
//! 嵌在 Panel 闭包里无法正确定位。

use super::*;
use crate::widgets::{ICON_SM, ROW_H};

/// 概况窗口的默认高度
///
/// egui 的 Window 不设 `default_height` 时，可绘制区高度是写死的 435px，
/// 内容超过就被裁——末尾的条目会掉出可视区、只剩上半截。
/// 620px 放得下当前全部条目，仍放不下时由内层 ScrollArea 承担滚动。
const INFO_WIN_MAX_H: f32 = 620.0;

/// 窗口上下留白，与 `window_margin` 一致
const WINDOW_MARGIN: f32 = 14.0;

/// 概况窗口的固定宽度
///
/// 两列布局：每格放得下「图标 + 取值」，标签靠 tip 承载。
/// 420px 下每格 172px，实测最长的「11.1 GB / 可回收 397.1 MB」（164px）能完整放下。
///
/// 必须用 `default_width` 钉死，不能只靠 `set_min_width`：Window 会记住上一帧尺寸，
/// 而格内标签按 `available_width` 定宽，于是「记住更宽 → 分更宽 → 再撑大」
/// 逐帧放大，实测 452 → 516 → 740px（与表格列宽的正反馈同源）。
const INFO_WIN_W: f32 = 420.0;

/// 单格宽度上限
///
/// 取值标签宽度只按它算，与窗口实际宽度解耦，切断上面的正反馈。
const CELL_MAX_W: f32 = 172.0;

/// 图标列宽
const ICON_W: f32 = 16.0;

/// 取值文本的亮色
const VALUE_COLOR: Color32 = Color32::from_rgb(240, 243, 248);

impl DvApp {
    /// 底部状态栏：左状态文本 + 右概况入口
    pub(in crate::app) fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.docker_info.is_some()
                    && icon_button(ui, ic::INFO, "Docker 概况", NEUTRAL, true)
                {
                    self.info_open = !self.info_open;
                }
                // 概览跟随当前 Tab：容器页讲容器，镜像页讲镜像
                let summary = match self.tab {
                    Tab::Containers => {
                        let running = self.containers.iter().filter(|c| c.is_running()).count();
                        format!("容器 {} · 运行中 {running}", self.containers.len())
                    }
                    Tab::Images => format!("镜像 {}", self.images.len()),
                };
                ui.weak(summary);
            });
        });
    }
}

/// Docker 概况窗口：引擎信息 + 资源总量 + 容器镜像计数 + 磁盘占用
pub(in crate::app) fn ui_docker_info(app: &mut DvApp, ctx: &egui::Context) {
    let mut open = app.info_open;
    let Some(info) = app.docker_info.clone() else {
        app.info_open = false;
        return;
    };
    egui::Window::new("Docker 概况")
        .id(egui::Id::new("docker_panel_docker_info"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .default_width(INFO_WIN_W)
        .default_height(INFO_WIN_MAX_H)
        .show(ctx, |ui| {
            // 内容整体套滚动区。
            //
            // 这里踩过两个坑，都实测过：
            //  1. Window::scroll(true) 无效——它不会把可绘制区随内容撑开，
            //     20 行内容加与不加都裁 17px；
            //  2. max_height 也无效——真正决定可绘制区高度的是 default_height，
            //     不设时恒为 435px（与屏幕高度无关），内容一过线就切掉末尾按钮。
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(INFO_WIN_MAX_H - 2.0 * WINDOW_MARGIN)
                .show(ui, |ui| {
                    section(ui, "Docker 引擎");
                    grid2(
                        ui,
                        &[
                            (ic::SERVER, "版本", info.server_version.clone()),
                            (ic::STORAGE, "存储驱动", info.driver.clone()),
                            (ic::INFO, "宿主系统", info.operating_system.clone()),
                        ],
                    );

                    section(ui, "主机资源");
                    grid2(
                        ui,
                        &[
                            (ic::SPEED, "CPU", format!("{} 核", info.ncpu)),
                            (ic::MEMORY, "内存", format_bytes(info.mem_total)),
                        ],
                    );

                    section(ui, "对象统计");
                    grid2(
                        ui,
                        &[
                            (ic::CONTAINERS, "容器", format!("{} 个", info.containers)),
                            (
                                ic::CHECK_ON,
                                "运行中",
                                format!("{} 个", info.containers_running),
                            ),
                            (
                                ic::PAUSE,
                                "已停止",
                                format!("{} 个", info.containers_stopped),
                            ),
                            (ic::IMAGES, "镜像", format!("{} 个", info.images)),
                        ],
                    );

                    section(ui, "磁盘占用");
                    grid2(
                        ui,
                        &[
                            (
                                ic::IMAGES,
                                "镜像占用",
                                reclaim_text(info.images_size, info.images_reclaimable),
                            ),
                            (
                                ic::CONTAINERS,
                                "容器占用",
                                format_bytes(info.containers_size),
                            ),
                            (ic::STORAGE, "卷占用", format_bytes(info.volumes_size)),
                            (
                                ic::BUILD,
                                "构建缓存",
                                reclaim_text(info.build_cache_size, info.build_cache_reclaimable),
                            ),
                        ],
                    );
                });
        });
    app.info_open = open;
}

/// 小节标题：标题 + 分隔线
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

/// 两列栅格：每格是「图标 + 取值」，标签只出现在悬浮提示里
///
/// 原先是「图标 + 标签 + 取值」一行一条，窗口又高又窄。改成两列后条目数不变、
/// 高度减半；标签下沉到 tip，省下的横向空间留给取值，宿主系统这类长文本不再挤。
fn grid2(ui: &mut egui::Ui, items: &[(char, &str, String)]) {
    const COLS: usize = 2;
    ui.columns(COLS, |cols| {
        for (i, (icon, tip, value)) in items.iter().enumerate() {
            let col = &mut cols[i % COLS];
            // 取值宽度按 CELL_MAX_W 封顶，不问 col.available_width()。
            //
            // 问 available_width 等于把「窗口当前多宽」写回内容宽度，
            // 而窗口尺寸是上一帧记下来的——两者互相放大，永不收敛
            // （实测 452 → 516 → 740px）。闭包里问 col 还会与借用冲突。
            let value_w = (col.available_width() - ICON_W).clamp(40.0, CELL_MAX_W);
            col.horizontal(|ui| {
                let icon_resp = ui.add_sized(
                    [ICON_W, ROW_H],
                    egui::Label::new(RichText::new(*icon).size(ICON_SM).color(TEXT_DIM)),
                );
                icon_resp.on_hover_text(*tip);
                ui.add_sized(
                    [value_w, ROW_H],
                    egui::Label::new(RichText::new(value).color(VALUE_COLOR)).truncate(),
                )
                .on_hover_text(format!("{tip}：{value}"));
            });
        }
    });
}

/// 已用大小 + 可回收量（可回收为 0 时只显示已用）
fn reclaim_text(used: u64, reclaimable: u64) -> String {
    if reclaimable > 0 {
        format!(
            "{} / 可回收 {}",
            format_bytes(used),
            format_bytes(reclaimable)
        )
    } else {
        format_bytes(used)
    }
}
