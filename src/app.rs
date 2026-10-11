//! egui App：全局状态、Tab 页面渲染、表格与各类弹窗
//!
//! 设计要点：
//! - 所有 Docker 调用都在 tokio 后台任务里执行，结果经 mpsc channel 回传，绝不阻塞 UI
//! - 行内按钮先收集动作，表格渲染完后统一执行，规避 egui 借用冲突
//! - 每帧轮询消息 + 定时刷新（容器 2s / 镜像 15s / 资源采样 2s）

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{pos2, Color32, FontId, Pos2, RichText, ScrollArea, Stroke};
use egui_extras::{Column, TableBuilder};
use futures::StreamExt;
use tokio::runtime::Runtime;

use crate::docker;
use crate::models::{ChartPoint, ContainerInfo, ImageInfo, LogEvent, Msg, ResourceSample};
use crate::utils::{format_age, format_bytes, load_cjk_fonts, state_label};

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

/// 二次确认弹窗
#[derive(Clone)]
enum Confirm {
    DeleteContainer { id: String, name: String },
    DeleteImage { name: String, tag: String },
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
    /// 历史采样点（最多保留 120 个 ≈ 4 分钟）
    history: Vec<ChartPoint>,
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

    containers_loading: bool,
    images_loading: bool,
    /// 未完成的资源采样数（用于限流）
    stats_pending: usize,

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
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
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
            containers_loading: false,
            images_loading: false,
            stats_pending: 0,
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

    // ---------- 后台任务发起 ----------

    /// 拉取容器列表
    fn refresh_containers(&mut self) {
        self.containers_loading = true;
        self.last_container_refresh = Instant::now();
        let tx = self.tx.clone();
        let all = self.filter == Filter::All;
        self.rt.spawn(async move {
            let msg = Msg::Containers(docker::list_containers(all).await);
            let _ = tx.send(msg);
        });
    }

    /// 拉取镜像列表
    fn refresh_images(&mut self) {
        self.images_loading = true;
        self.last_image_refresh = Instant::now();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let msg = Msg::Images(docker::list_images().await);
            let _ = tx.send(msg);
        });
    }

    /// 采样所有运行中容器的资源占用（并发上限 4）
    fn refresh_stats(&mut self) {
        self.last_stats = Instant::now();
        let mut ids: Vec<String> = self
            .containers
            .iter()
            .filter(|c| c.is_running())
            .map(|c| c.id.clone())
            .collect();
        if let Some(rv) = &self.resource_view {
            if !ids.contains(&rv.id) {
                ids.push(rv.id.clone());
            }
        }
        if ids.is_empty() {
            return;
        }
        self.stats_pending = ids.len();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let tasks = futures::stream::iter(ids.into_iter().map(|id| async move {
                let r = docker::sample_container(&id).await;
                (id, r)
            }))
            .buffer_unordered(4);
            let results: Vec<_> = tasks.collect().await;
            for (id, r) in results {
                if tx.send(Msg::Sample(id, r)).is_err() {
                    break;
                }
            }
        });
    }

    /// 执行容器操作（启动/停止/重启/删除）
    fn spawn_action(&mut self, kind: &'static str, id: String, name: String) {
        let tx = self.tx.clone();
        let desc = format!("{kind} {name}");
        self.rt.spawn(async move {
            let r = match kind {
                "启动" => docker::start_container(&id).await,
                "停止" => docker::stop_container(&id).await,
                "重启" => docker::restart_container(&id).await,
                "删除" => docker::remove_container(&id).await,
                _ => Err("未知操作".into()),
            };
            let _ = tx.send(Msg::ActionDone(desc, r));
        });
    }

    /// 拉取镜像
    fn start_pull(&mut self) {
        let spec = self.pull_spec.trim().to_string();
        if spec.is_empty() || self.pull_busy {
            return;
        }
        self.pull_busy = true;
        self.pull_progress.clear();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let progress_tx = tx.clone();
            let r = docker::pull_image(&spec, move |text| {
                let _ = progress_tx.send(Msg::PullProgress(text));
            })
            .await;
            let _ = tx.send(Msg::PullDone(r));
        });
    }

    /// 打开日志窗口（后台持续读日志流）
    fn open_log(&mut self, id: String, name: String) {
        self.close_log();
        let (tx, rx) = channel();
        let task_id = id.clone();
        let handle = self.rt.spawn(async move {
            let _ = docker::follow_logs(&task_id, 200, tx).await;
        });
        self.log_view = Some(LogView {
            id,
            name,
            lines: Vec::new(),
            follow: true,
            ended: false,
            rx,
            handle: Some(handle),
        });
    }

    /// 关闭日志窗口并中断日志流任务
    fn close_log(&mut self) {
        if let Some(mut lv) = self.log_view.take() {
            if let Some(h) = lv.handle.take() {
                h.abort();
            }
        }
    }

    /// 打开/切换资源监控窗口
    fn open_resource(&mut self, id: String, name: String) {
        match &self.resource_view {
            Some(rv) if rv.id == id => {}
            _ => {
                self.resource_view = Some(ResourceView {
                    id,
                    name,
                    history: Vec::new(),
                });
                self.refresh_stats();
            }
        }
    }

    // ---------- 消息处理 ----------

    /// 轮询后台消息队列
    fn pump_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Containers(Ok(list)) => {
                    self.containers_loading = false;
                    // 保留上一次的资源数据，避免刷新瞬间指标归零
                    let old: HashMap<String, ContainerInfo> = self
                        .containers
                        .iter()
                        .map(|c| (c.id.clone(), c.clone()))
                        .collect();
                    self.containers = list
                        .into_iter()
                        .map(|mut c| {
                            if let Some(prev) = old.get(&c.id) {
                                c.cpu_percent = prev.cpu_percent;
                                c.mem_usage = prev.mem_usage;
                                c.mem_limit = prev.mem_limit;
                                c.block_read = prev.block_read;
                                c.block_write = prev.block_write;
                                c.net_rx = prev.net_rx;
                                c.net_tx = prev.net_tx;
                            }
                            c
                        })
                        .collect();
                    self.status = format!("容器 {} 个", self.containers.len());
                }
                Msg::Containers(Err(e)) => {
                    self.containers_loading = false;
                    self.show_error(e);
                }
                Msg::Images(Ok(list)) => {
                    self.images_loading = false;
                    self.images = list;
                }
                Msg::Images(Err(e)) => {
                    self.images_loading = false;
                    self.show_error(e);
                }
                Msg::Sample(id, Ok(sample)) => {
                    self.stats_pending = self.stats_pending.saturating_sub(1);
                    self.apply_sample(id, sample);
                }
                Msg::Sample(_, Err(_)) => {
                    // 容器可能刚好停止，忽略采样失败
                    self.stats_pending = self.stats_pending.saturating_sub(1);
                }
                Msg::PullProgress(text) => {
                    self.pull_progress.push(text);
                    if self.pull_progress.len() > 200 {
                        self.pull_progress.drain(..100);
                    }
                }
                Msg::PullDone(r) => {
                    self.pull_busy = false;
                    match r {
                        Ok(msg) => {
                            self.pull_spec.clear();
                            self.status = msg.clone();
                            self.result = Some(msg);
                            self.refresh_images();
                        }
                        Err(e) => self.show_error(e),
                    }
                }
                Msg::ActionDone(desc, r) => match r {
                    Ok(msg) => {
                        self.status = format!("{desc}: {msg}");
                        self.refresh_containers();
                        self.refresh_images();
                    }
                    Err(e) => self.show_error(e),
                },
                Msg::CleanupDone(r) => match r {
                    Ok(summary) => {
                        self.status = "清理完成".into();
                        self.result = Some(summary);
                        self.refresh_containers();
                        self.refresh_images();
                    }
                    Err(e) => self.show_error(e),
                },
            }
        }
    }

    /// 把采样结果应用到表格行与监控曲线
    fn apply_sample(&mut self, id: String, s: ResourceSample) {
        if let Some(c) = self.containers.iter_mut().find(|c| c.id == id) {
            c.cpu_percent = s.cpu_percent;
            c.mem_usage = s.mem_usage;
            c.mem_limit = s.mem_limit;
            c.block_read = s.block_read;
            c.block_write = s.block_write;
            c.net_rx = s.net_rx;
            c.net_tx = s.net_tx;
        }
        if let Some(rv) = self.resource_view.as_mut().filter(|rv| rv.id == id) {
            rv.history.push(ChartPoint {
                cpu: s.cpu_percent,
                mem_usage: s.mem_usage,
                mem_limit: s.mem_limit,
            });
            if rv.history.len() > 120 {
                rv.history.remove(0);
            }
        }
    }

    /// 记录日志流事件
    fn poll_logs(&mut self) {
        if let Some(lv) = self.log_view.as_mut() {
            while let Ok(ev) = lv.rx.try_recv() {
                match ev {
                    LogEvent::Line(line) => {
                        lv.lines.push(line);
                        if lv.lines.len() > 5000 {
                            lv.lines.drain(..500);
                        }
                    }
                    LogEvent::Closed => lv.ended = true,
                }
            }
        }
    }

    /// 同一错误不重复弹窗
    fn show_error(&mut self, e: String) {
        if e != self.last_error {
            self.last_error = e.clone();
            self.error = Some(e);
        }
    }

    /// 定时刷新容器/镜像/资源采样
    fn maybe_refresh(&mut self) {
        if !self.containers_loading
            && self.last_container_refresh.elapsed() >= Duration::from_secs(2)
        {
            self.refresh_containers();
        }
        if self.tab == Tab::Images
            && !self.images_loading
            && self.last_image_refresh.elapsed() >= Duration::from_secs(15)
        {
            self.refresh_images();
        }
        if self.stats_pending == 0 && self.last_stats.elapsed() >= Duration::from_secs(2) {
            self.refresh_stats();
        }
    }

    // ---------- 渲染 ----------

    /// 顶部 Tab 栏
    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Containers, "Containers 容器");
            ui.selectable_value(&mut self.tab, Tab::Images, "Images 镜像");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(&self.status).weak());
            });
        });
    }

    /// 容器 Tab
    fn ui_containers(&mut self, ui: &mut egui::Ui) {
        // 工具条：筛选 + 手动刷新 + 清理入口
        let mut need_refresh = false;
        ui.horizontal(|ui| {
            ui.label("筛选:");
            ui.selectable_value(&mut self.filter, Filter::Running, "仅正在运行");
            ui.selectable_value(&mut self.filter, Filter::All, "全部容器");
            if ui.button("刷新").clicked() {
                need_refresh = true;
            }
            if ui.button("清理...").clicked() {
                self.cleanup_open = true;
            }
            if self.containers_loading {
                ui.spinner();
            }
        });
        if need_refresh {
            self.refresh_containers();
        }

        // 筛选后的行数据先拷贝，避免渲染期间借用冲突
        let rows: Vec<ContainerInfo> = self
            .containers
            .iter()
            .filter(|c| match self.filter {
                Filter::Running => c.is_running(),
                Filter::All => true,
            })
            .cloned()
            .collect();

        let mut actions: Vec<RowAction> = Vec::new();

        ScrollArea::horizontal().show(ui, |ui| {
            ui.set_min_width(1420.0);
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .vscroll(false)
                .column(Column::initial(95.0).at_least(80.0)) // 短ID
                .column(Column::initial(150.0).at_least(100.0)) // 名称
                .column(Column::initial(180.0).at_least(120.0)) // 镜像
                .column(Column::initial(80.0).at_least(70.0)) // 状态
                .column(Column::initial(130.0).at_least(90.0)) // 运行时长
                .column(Column::initial(200.0).at_least(120.0)) // 端口映射
                .column(Column::initial(75.0).at_least(65.0)) // CPU
                .column(Column::initial(140.0).at_least(100.0)) // 内存
                .column(Column::initial(130.0).at_least(100.0)) // 磁盘读写
                .column(Column::initial(140.0).at_least(100.0)) // 网络IO
                .column(Column::remainder().at_least(270.0)) // 操作
                .header(26.0, |mut header| {
                    for name in [
                        "短ID",
                        "容器名称",
                        "镜像",
                        "状态",
                        "运行时长",
                        "端口映射",
                        "CPU",
                        "内存占用",
                        "磁盘读写",
                        "网络IO",
                        "操作",
                    ] {
                        header.col(|ui| {
                            ui.strong(name);
                        });
                    }
                })
                .body(|body| {
                    body.rows(28.0, rows.len(), |mut row| {
                        let c = &rows[row.index()];
                        let running = c.is_running();
                        let id = c.id.clone();
                        let name = c.name.clone();

                        row.col(|ui| {
                            ui.monospace(c.short_id());
                        });
                        row.col(|ui| {
                            ui.label(&c.name);
                        });
                        row.col(|ui| {
                            ui.label(&c.image);
                        });
                        row.col(|ui| {
                            let (text, color) = state_label(&c.state);
                            ui.colored_label(color, text);
                        });
                        row.col(|ui| {
                            let text = if c.status.is_empty() {
                                "-".to_string()
                            } else {
                                c.status.clone()
                            };
                            ui.label(text);
                        });
                        row.col(|ui| {
                            if c.ports.is_empty() {
                                ui.strong("-");
                            } else {
                                ui.label(&c.ports);
                            }
                        });
                        row.col(|ui| {
                            if running {
                                let color = if c.cpu_percent >= 80.0 {
                                    Color32::from_rgb(231, 76, 60)
                                } else {
                                    ui.visuals().text_color()
                                };
                                ui.colored_label(color, format!("{:.1}%", c.cpu_percent));
                            } else {
                                ui.weak("-");
                            }
                        });
                        row.col(|ui| {
                            if running && c.mem_limit > 0 {
                                ui.label(format!(
                                    "{} ({:.1}%)",
                                    format_bytes(c.mem_usage),
                                    c.mem_percent()
                                ));
                            } else if running {
                                ui.label(format_bytes(c.mem_usage));
                            } else {
                                ui.weak("-");
                            }
                        });
                        row.col(|ui| {
                            if running {
                                ui.label(format!(
                                    "R {} / W {}",
                                    format_bytes(c.block_read),
                                    format_bytes(c.block_write)
                                ));
                            } else {
                                ui.weak("-");
                            }
                        });
                        row.col(|ui| {
                            if running {
                                ui.label(format!(
                                    "↓ {} / ↑ {}",
                                    format_bytes(c.net_rx),
                                    format_bytes(c.net_tx)
                                ));
                            } else {
                                ui.weak("-");
                            }
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if ui.small_button("日志").clicked() {
                                    actions.push(RowAction::Logs(id.clone(), name.clone()));
                                }
                                if ui.small_button("监控").clicked() {
                                    actions.push(RowAction::Stats(id.clone(), name.clone()));
                                }
                                if ui
                                    .add_enabled(
                                        !running,
                                        egui::Button::new("启动").small(),
                                    )
                                    .clicked()
                                {
                                    actions.push(RowAction::Start(id.clone(), name.clone()));
                                }
                                if ui
                                    .add_enabled(
                                        running,
                                        egui::Button::new("停止").small(),
                                    )
                                    .clicked()
                                {
                                    actions.push(RowAction::Stop(id.clone(), name.clone()));
                                }
                                if ui
                                    .add_enabled(
                                        running,
                                        egui::Button::new("重启").small(),
                                    )
                                    .clicked()
                                {
                                    actions.push(RowAction::Restart(id.clone(), name.clone()));
                                }
                                if ui.small_button("删除").clicked() {
                                    actions.push(RowAction::DeleteContainer(id.clone(), name));
                                }
                            });
                        });
                    });
                });
        });

        // 表格渲染完毕后统一执行动作
        for action in actions {
            match action {
                RowAction::Logs(id, name) => self.open_log(id, name),
                RowAction::Stats(id, name) => self.open_resource(id, name),
                RowAction::Start(id, name) => self.spawn_action("启动", id, name),
                RowAction::Stop(id, name) => self.spawn_action("停止", id, name),
                RowAction::Restart(id, name) => self.spawn_action("重启", id, name),
                RowAction::DeleteContainer(id, name) => {
                    self.confirm = Some(Confirm::DeleteContainer { id, name });
                }
                RowAction::DeleteImage(name, tag) => {
                    self.confirm = Some(Confirm::DeleteImage { name, tag });
                }
            }
        }

        if rows.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("没有符合条件的容器");
            });
        }
    }

    /// 镜像 Tab
    fn ui_images(&mut self, ui: &mut egui::Ui) {
        let mut need_refresh = false;
        let mut need_pull = false;

        // 工具条：拉取输入框 + 刷新 + 清理入口
        ui.horizontal(|ui| {
            ui.label("拉取镜像:");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.pull_spec)
                    .hint_text("例如 nginx:latest")
                    .desired_width(260.0),
            );
            let enter_pressed =
                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("拉取").clicked() || enter_pressed) && !self.pull_busy {
                need_pull = true;
            }
            if self.pull_busy {
                ui.spinner();
            }
            if ui.button("刷新").clicked() {
                need_refresh = true;
            }
            if ui.button("清理...").clicked() {
                self.cleanup_open = true;
            }
        });

        // 拉取进度（最后一行状态）
        if let Some(last) = self.pull_progress.last() {
            ui.label(RichText::new(last).weak().monospace());
        }

        if need_pull {
            self.start_pull();
        }
        if need_refresh {
            self.refresh_images();
        }

        let rows: Vec<ImageInfo> = self.images.clone();
        let mut actions: Vec<RowAction> = Vec::new();

        ScrollArea::horizontal().show(ui, |ui| {
            ui.set_min_width(760.0);
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .vscroll(false)
                .column(Column::initial(240.0).at_least(150.0)) // 镜像名称
                .column(Column::initial(130.0).at_least(80.0)) // Tag
                .column(Column::initial(150.0).at_least(110.0)) // 大小
                .column(Column::initial(140.0).at_least(100.0)) // 创建时间
                .column(Column::initial(110.0).at_least(80.0)) // 关联容器
                .column(Column::remainder().at_least(90.0)) // 操作
                .header(26.0, |mut header| {
                    for name in ["镜像名称", "Tag", "镜像大小", "创建时间", "关联容器", "操作"] {
                        header.col(|ui| {
                            ui.strong(name);
                        });
                    }
                })
                .body(|body| {
                    body.rows(28.0, rows.len(), |mut row| {
                        let img = &rows[row.index()];
                        row.col(|ui| {
                            let tip = format!("{} · {}", img.short_id(), img.id);
                            let text = if img.dangling {
                                egui::RichText::new(&img.name)
                                    .color(Color32::from_rgb(241, 196, 15))
                            } else {
                                egui::RichText::new(&img.name)
                            };
                            ui.label(text).on_hover_text(tip);
                        });
                        row.col(|ui| {
                            ui.monospace(&img.tag);
                        });
                        row.col(|ui| {
                            ui.label(format_bytes(img.size as u64));
                        });
                        row.col(|ui| {
                            ui.label(format_age(img.created));
                        });
                        row.col(|ui| {
                            ui.label(format!("{}", img.containers));
                        });
                        row.col(|ui| {
                            let name = img.name.clone();
                            let tag = img.tag.clone();
                            if ui.small_button("删除").clicked() {
                                actions.push(RowAction::DeleteImage(name, tag));
                            }
                        });
                    });
                });
        });

        for action in actions {
            if let RowAction::DeleteImage(name, tag) = action {
                self.confirm = Some(Confirm::DeleteImage { name, tag });
            }
        }

        if rows.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("暂无镜像，可在上方输入镜像名拉取");
            });
        }
    }

    /// 日志窗口
    fn ui_log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.log_view.is_some();
        if let Some(lv) = self.log_view.as_mut() {
            let short = lv.id.chars().take(12).collect::<String>();
            egui::Window::new(format!("容器日志 — {} ({short})", lv.name))
                .id(egui::Id::new("dvkit_log_window"))
                .open(&mut open)
                .default_width(800.0)
                .default_height(480.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let btn = if lv.follow {
                            "暂停滚动"
                        } else {
                            "继续滚动"
                        };
                        if ui.button(btn).clicked() {
                            lv.follow = !lv.follow;
                        }
                        if ui.button("清空日志").clicked() {
                            lv.lines.clear();
                        }
                        if lv.ended {
                            ui.weak("日志流已结束");
                        } else {
                            ui.spinner();
                            ui.weak("实时接收中");
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.weak(format!("{} 行", lv.lines.len()));
                            },
                        );
                    });
                    ui.separator();
                    let row_height =
                        ui.text_style_height(&egui::TextStyle::Monospace);
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(lv.follow)
                        .show_rows(
                            ui,
                            row_height,
                            lv.lines.len(),
                            |ui, range| {
                                for line in &lv.lines[range] {
                                    ui.monospace(line);
                                }
                            },
                        );
                });
        }
        if !open {
            self.close_log();
        }
    }

    /// 资源监控窗口（CPU / 内存折线小图）
    fn ui_resource_window(&mut self, ctx: &egui::Context) {
        let mut open = self.resource_view.is_some();
        if let Some(rv) = self.resource_view.as_mut() {
            let latest = rv.history.last().copied();
            egui::Window::new(format!("资源监控 — {}", rv.name))
                .id(egui::Id::new("dvkit_resource_window"))
                .open(&mut open)
                .default_width(520.0)
                .show(ctx, |ui| {
                    if let Some(p) = latest {
                        ui.horizontal(|ui| {
                            ui.label(format!("CPU: {:.1}%", p.cpu));
                            ui.separator();
                            ui.label(format!(
                                "内存: {} / {} ({:.1}%)",
                                format_bytes(p.mem_usage),
                                format_bytes(p.mem_limit),
                                if p.mem_limit > 0 {
                                    p.mem_usage as f64 / p.mem_limit as f64 * 100.0
                                } else {
                                    0.0
                                }
                            ));
                        });
                    } else {
                        ui.weak("采样中...（仅运行中的容器有数据）");
                    }
                    ui.separator();

                    // CPU 曲线
                    let cpu_values: Vec<f64> =
                        rv.history.iter().map(|p| p.cpu).collect();
                    let cpu_max = cpu_values
                        .iter()
                        .cloned()
                        .fold(10.0f64, f64::max)
                        * 1.2;
                    ui.label("CPU 使用率 (%)");
                    draw_chart(
                        ui,
                        &cpu_values,
                        cpu_max,
                        Color32::from_rgb(52, 152, 219),
                    );

                    ui.add_space(8.0);
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
                    ui.label("内存占用 (%)");
                    draw_chart(
                        ui,
                        &mem_values,
                        100.0,
                        Color32::from_rgb(46, 204, 113),
                    );
                });
        }
        if !open {
            self.resource_view = None;
        }
    }

    /// 清理弹窗（勾选 + 执行前确认）
    fn ui_cleanup_window(&mut self, ctx: &egui::Context) {
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
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("请选择要清理的内容：");
                ui.add_space(4.0);
                ui.checkbox(&mut self.cleanup_stopped, "清理已停止的容器");
                ui.checkbox(&mut self.cleanup_dangling, "清理悬空镜像（dangling）");
                ui.checkbox(&mut self.cleanup_unused, "清理未使用镜像（所有未被容器使用的镜像）");
                ui.add_space(8.0);
                ui.colored_label(
                    Color32::from_rgb(241, 196, 15),
                    "⚠ 执行后不可恢复，确认要继续吗？",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let any = self.cleanup_stopped
                        || self.cleanup_dangling
                        || self.cleanup_unused;
                    if ui
                        .add_enabled(any, egui::Button::new("确认执行"))
                        .clicked()
                    {
                        decided = Some(true);
                    }
                    if ui.button("取消").clicked() {
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

    /// 二次确认弹窗
    fn ui_confirm_window(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.confirm.clone() else {
            return;
        };
        let (title, body) = match &confirm {
            Confirm::DeleteContainer { name, .. } => (
                "删除容器",
                format!("确定要删除容器 {name} 吗？\n运行中的容器会被强制删除，此操作不可恢复。"),
            ),
            Confirm::DeleteImage { name, tag } => (
                "删除镜像",
                format!(
                    "确定要删除镜像 {name}:{tag} 吗？\n被容器引用的镜像无法删除，此操作不可恢复。"
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
            .show(ctx, |ui| {
                for line in body.lines() {
                    ui.label(line);
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("确认删除").clicked() {
                        accepted = true;
                        open = false;
                    }
                    if ui.button("取消").clicked() {
                        open = false;
                    }
                });
            });
        if !open {
            self.confirm = None;
            if accepted {
                match confirm {
                    Confirm::DeleteContainer { id, name } => {
                        self.spawn_action("删除", id, name)
                    }
                    Confirm::DeleteImage { name, tag } => {
                        let tx = self.tx.clone();
                        let desc = format!("删除 {name}:{tag}");
                        self.rt.spawn(async move {
                            let r = docker::remove_image(&name, &tag).await;
                            let _ = tx.send(Msg::ActionDone(desc, r));
                        });
                    }
                }
            }
        }
    }

    /// 错误弹窗（Docker API 调用失败）
    fn ui_error_window(&mut self, ctx: &egui::Context) {
        let Some(err) = self.error.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Docker 调用失败")
            .id(egui::Id::new("dvkit_error_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.colored_label(Color32::from_rgb(231, 76, 60), &err);
                ui.add_space(10.0);
                if ui.button("确定").clicked() {
                    open = false;
                }
            });
        if !open {
            self.error = None;
            self.last_error.clear();
        }
    }

    /// 信息弹窗（操作/清理结果）
    fn ui_result_window(&mut self, ctx: &egui::Context) {
        let Some(text) = self.result.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("执行结果")
            .id(egui::Id::new("dvkit_result_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                for line in text.lines() {
                    ui.label(line);
                }
                ui.add_space(10.0);
                if ui.button("确定").clicked() {
                    open = false;
                }
            });
        if !open {
            self.result = None;
        }
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
        egui::Panel::top("dvkit_tabs").show(ui, |ui| {
            ui.add_space(4.0);
            self.ui_tabs(ui);
            ui.add_space(4.0);
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

/// 简易折线小图（values 为序列，超出 max 的按 max 截断）
fn draw_chart(ui: &mut egui::Ui, values: &[f64], max: f64, color: Color32) {
    let height = 110.0;
    let desired = egui::vec2(ui.available_width(), height);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // 背景与网格
    painter.rect_filled(rect, 4.0, Color32::from_gray(26));
    for i in 1..4 {
        let y = rect.top() + rect.height() * i as f32 / 4.0;
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_gray(45)),
        );
    }

    // 数据曲线
    if values.len() >= 2 && max > 0.0 {
        let n = values.len();
        let points: Vec<Pos2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
                let ratio = (v / max).clamp(0.0, 1.0) as f32;
                let y = rect.bottom() - ratio * rect.height();
                pos2(x, y)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(1.8, color)));
    }

    // 当前值标注
    if let Some(last) = values.last() {
        painter.text(
            rect.right_top() - egui::vec2(8.0, -4.0),
            egui::Align2::RIGHT_TOP,
            format!("{last:.1}"),
            FontId::proportional(11.0),
            Color32::LIGHT_GRAY,
        );
    }
}
