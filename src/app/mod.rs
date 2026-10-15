//! egui App：全局状态、生命周期与帧调度
//!
//! 子模块按职责拆分：
//! - `types`：共享枚举/结构体（Tab、RowAction、弹窗/视图状态）
//! - `tasks`：后台任务发起与 channel 消息泵
//! - `containers` / `images`：两个 Tab 的表格渲染
//! - `windows`：日志、资源监控、清理、确认等弹窗
//!
//! 设计要点：
//! - 所有 Docker 调用都在 tokio 后台任务里执行，结果经 mpsc channel 回传，绝不阻塞 UI
//! - 行内按钮先收集动作，表格渲染完后统一执行，规避 egui 借用冲突
//! - 每帧轮询消息 + 定时刷新（容器 2s / 镜像 15s / 资源采样 2s）

mod columns;
mod constants;
mod containers;
mod images;
mod tasks;
mod types;
mod windows;

use columns::*;
use constants::*;
use tasks::ActionKind;
use types::*;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, FontId, RichText, ScrollArea, Stroke};
use egui_extras::{Column, TableBuilder};
use futures::StreamExt;
use tokio::runtime::Runtime;

use crate::docker;
use crate::models::{ChartPoint, ContainerInfo, ImageInfo, LogEvent, Msg, ResourceSample};
use crate::utils::{
    apply_theme, format_age, format_bytes, load_cjk_fonts, state_label, ACCENT, AMBER, BLUE, CYAN,
    DIVIDER, GREEN, PANEL_BG, RED, SURFACE_BG, TEXT_DIM,
};
use crate::widgets::*;

/// 应用主状态
pub struct DvApp {
    /// tokio 运行时（后台执行 Docker 调用）
    rt: Runtime,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    tab: Tab,
    filter: Filter,
    containers: Vec<ContainerInfo>,
    images: Vec<ImageInfo>,

    /// 容器搜索关键词（即时过滤名称/ID/镜像/端口/状态）
    container_query: String,
    /// 镜像搜索关键词
    image_query: String,
    /// 多选的容器 ID
    selected_containers: HashSet<String>,
    /// 多选的镜像（"名称:tag"）
    selected_images: HashSet<String>,

    containers_loading: bool,
    images_loading: bool,
    /// Docker 连接状态（None=尚未探测，Some(true)=正常，Some(false)=异常）
    /// 用它给顶部角标着色，避免刷新期间颜色来回跳
    docker_ok: Option<bool>,
    /// 未完成的资源采样数（用于限流）
    stats_pending: usize,

    // ---- 列表展示控制 ----
    /// 当前显示的容器列（CtlCol::ALL 的子集，选择列恒显示）
    ctl_cols: Vec<CtlCol>,
    /// 当前显示的镜像列
    img_cols: Vec<ImgCol>,
    /// 容器表排序（列, 是否升序）
    ctl_sort: (CtlCol, bool),
    /// 镜像表排序（列, 是否升序）
    img_sort: (ImgCol, bool),
    /// 是否按 compose 项目分组展示
    group_compose: bool,
    /// 已折叠的 compose 组（key 为项目名）
    collapsed_groups: HashSet<String>,

    pull_spec: String,
    pull_busy: bool,
    pull_progress: Vec<String>,

    /// 二次确认弹窗
    confirm: Option<Confirm>,
    /// 普通信息弹窗
    result: Option<String>,
    /// 错误弹窗
    error: Option<String>,
    /// 上次错误内容（避免重复弹同一个错）
    last_error: String,

    /// 清理弹窗
    cleanup_open: bool,
    cleanup_stopped: bool,
    cleanup_dangling: bool,
    cleanup_unused: bool,

    log_view: Option<LogView>,
    resource_view: Option<ResourceView>,

    last_container_refresh: Instant,
    last_image_refresh: Instant,
    last_stats: Instant,
    /// 底部状态栏文本
    status: String,
}

impl DvApp {
    /// 初始化：暗色主题、中文字体、tokio 运行时、首次拉取数据
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        load_cjk_fonts(&cc.egui_ctx);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("创建 tokio 运行时失败");
        let (tx, rx) = channel();

        let mut app = Self {
            rt,
            tx,
            rx,
            tab: Tab::Containers,
            filter: Filter::Running,
            containers: Vec::new(),
            images: Vec::new(),
            container_query: String::new(),
            image_query: String::new(),
            selected_containers: HashSet::new(),
            selected_images: HashSet::new(),
            containers_loading: false,
            images_loading: false,
            docker_ok: None,
            stats_pending: 0,
            ctl_cols: CtlCol::ALL.to_vec(),
            img_cols: ImgCol::ALL.to_vec(),
            ctl_sort: (CtlCol::Name, true),
            img_sort: (ImgCol::Created, false),
            group_compose: false,
            collapsed_groups: HashSet::new(),
            pull_spec: String::new(),
            pull_busy: false,
            pull_progress: Vec::new(),
            confirm: None,
            result: None,
            error: None,
            last_error: String::new(),
            cleanup_open: false,
            cleanup_stopped: true,
            cleanup_dangling: true,
            cleanup_unused: false,
            log_view: None,
            resource_view: None,
            last_container_refresh: Instant::now() - Duration::from_secs(INITIAL_REFRESH_LEAD_SECS),
            last_image_refresh: Instant::now() - Duration::from_secs(INITIAL_REFRESH_LEAD_SECS),
            last_stats: Instant::now(),
            status: "正在连接 Docker...".into(),
        };
        app.refresh_containers();
        app.refresh_images();
        app
    }
}

impl eframe::App for DvApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 1. 消息与定时器
        self.pump_messages();
        self.poll_logs();
        self.maybe_refresh();

        // 1.5 全局键盘快捷键
        ctx.input(|i| {
            // F5 — 刷新当前 tab
            if i.key_pressed(egui::Key::F5) {
                match self.tab {
                    Tab::Containers => self.refresh_containers(),
                    Tab::Images => self.refresh_images(),
                }
            }
            // Ctrl+1 / Ctrl+2 — 切换 tab
            if i.modifiers.command && i.key_pressed(egui::Key::Num1) {
                self.tab = Tab::Containers;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Num2) {
                self.tab = Tab::Images;
            }
            // Ctrl+Tab — 循环切换
            if i.modifiers.command && i.key_pressed(egui::Key::Tab) {
                self.tab = match self.tab {
                    Tab::Containers => Tab::Images,
                    Tab::Images => Tab::Containers,
                };
            }
        });

        // 2. 主界面
        egui::Panel::top("docker_panel_tabs")
            .frame(
                egui::Frame::default()
                    .fill(DEEP_BG)
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .stroke(Stroke::new(1.0, DIVIDER)),
            )
            .show(ui, |ui| {
                self.ui_header(ui);
            });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Containers => self.ui_containers(ui),
            Tab::Images => self.ui_images(ui),
        });

        // 3. 各类弹窗
        self.ui_log_window(&ctx);
        self.ui_resource_window(&ctx);
        self.ui_cleanup_window(&ctx);
        self.ui_confirm_window(&ctx);
        self.ui_error_window(&ctx);
        self.ui_result_window(&ctx);

        // 4. 无系统装饰时，自己画一圈外框让窗口在桌面里有边界感
        //    用 ctx.screen_rect() 拿整个窗口的范围（含 TopBottomPanel），
        //    再通过 LayerId::background 确保在最底层绘制不被面板遮挡
        let screen = ctx.viewport_rect();
        ctx.layer_painter(egui::LayerId::background()).rect_stroke(
            screen,
            CornerRadius::same(10),
            Stroke::new(1.0, DIVIDER),
            egui::epaint::StrokeKind::Inside,
        );

        // 5. 自适应重绘：交互中立即重绘保证 60fps，空闲才退回定时器
        let active_interaction = ctx.input(|i| {
            i.pointer.is_decidedly_dragging()
                || i.pointer.velocity().length() > 0.5
                || i.pointer.any_pressed()
                || i.pointer.any_released()
        });
        if active_interaction {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(REPAINT_INTERVAL_MS));
        }
    }
}
