//! egui App：全局状态、生命周期与帧调度
//!
//! 子模块按职责拆分：
//! - `tasks`：后台任务发起与 channel 消息泵
//! - `containers` / `images`：两个 Tab 的表格渲染
//! - `windows`：日志、资源监控、清理、确认等弹窗
//!
//! 设计要点：
//! - 所有 Docker 调用都在 tokio 后台任务里执行，结果经 mpsc channel 回传，绝不阻塞 UI
//! - 行内按钮先收集动作，表格渲染完后统一执行，规避 egui 借用冲突
//! - 每帧轮询消息 + 定时刷新（容器 2s / 镜像 15s / 资源采样 2s）

mod columns;
mod containers;
mod images;
mod tasks;
mod windows;

use columns::*;
use tasks::ActionKind;

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

/// 顶部 Tab
#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Containers,
    Images,
}

/// 容器筛选条件
#[derive(PartialEq, Eq, Clone, Copy)]
enum Filter {
    Running,
    All,
}

/// 表格行内待执行的动作（渲染结束后统一执行）
enum RowAction {
    Logs(String, String),
    Stats(String, String),
    Start(String, String),
    Stop(String, String),
    Restart(String, String),
    DeleteContainer(String, String),
    DeleteImage(String, String),
}

/// 二次确认弹窗（变体名统一以 Delete 开头，便于一眼读出弹窗性质）
#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
enum Confirm {
    DeleteContainer {
        id: String,
        name: String,
    },
    DeleteImage {
        name: String,
        tag: String,
    },
    /// 批量删除容器（items: (id, 名称)）
    DeleteContainers {
        items: Vec<(String, String)>,
    },
    /// 批量删除镜像（items: (名称, tag)）
    DeleteImages {
        items: Vec<(String, String)>,
    },
}

/// 日志查看窗口状态
struct LogView {
    id: String,
    name: String,
    lines: Vec<String>,
    /// 是否跟随滚动到底部
    follow: bool,
    /// 日志流是否已结束
    ended: bool,
    rx: Receiver<LogEvent>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

/// 资源监控窗口状态
struct ResourceView {
    id: String,
    name: String,
    /// 历史采样点（最多保留 120 个 ≈ 4 分钟；VecDeque 便于头部淘汰）
    history: VecDeque<ChartPoint>,
}

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
            last_container_refresh: Instant::now() - Duration::from_secs(10),
            last_image_refresh: Instant::now() - Duration::from_secs(10),
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

        // 2. 主界面
        egui::Panel::top("dvkit_tabs")
            .frame(
                egui::Frame::default()
                    .fill(DEEP_BG)
                    .inner_margin(egui::Margin::symmetric(14, 8))
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

        // 4. 保证后台消息被及时处理
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
