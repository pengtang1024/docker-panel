//! UI 状态 + 消息处理
//!
//! 线程模型（修死锁版）：
//! - AppState（Arc<Mutex<>>）只放**可共享**的状态：tokio runtime、tx、列表数据、query
//! - mpsc::Receiver<Msg> **不在 Mutex 内** — consumer 线程独占它
//!   → consumer 的 recv() 永远不阻塞主线程拿 state 锁
//! - 详情字段格式化见 detail_fmt.rs；consumer 泵见 consumer.rs

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;

use crate::batch::BatchState;
use crate::docker::{
    compute_cpu_delta, inspect_container, list_containers, list_images, list_networks,
    list_volumes, ContainerStatsSnapshot,
};
use crate::models::{
    ContainerDetail, ContainerInfo, ImageInfo, Msg, NetworkInfo, SortField, VolumeInfo,
};

/// 抽屉当前展示的资源类型（决定详情区渲染哪份数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelKind {
    #[default]
    Container,
    Image,
    Network,
    Volume,
}

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
/// 卷磁盘占用刷新周期（`/system/df` 会遍历卷目录统计，不能跟着 3s 列表刷新跑）
pub const VOLUME_SIZE_INTERVAL: Duration = Duration::from_secs(30);

/// 后台可共享状态
/// 注意：mpsc::Receiver<Msg> 不在这里 — 它由 consumer 线程独占
pub struct AppState {
    pub rt: Runtime,
    pub tx: mpsc::Sender<Msg>,

    pub containers: Vec<ContainerInfo>,
    pub images: Vec<ImageInfo>,
    pub networks: Vec<NetworkInfo>,
    pub volumes: Vec<VolumeInfo>,

    pub last_refresh: Instant,
    pub container_query: String,
    pub image_query: String,

    // —— Phase 2 详情面板 ——
    pub detail: Option<ContainerDetail>,
    // —— 镜像详情面板 ——
    pub image_detail: Option<crate::models::ImageDetail>,
    /// 抽屉当前选中的资源类型与主键（容器 id / 镜像 key / 网络 id / 卷名）
    pub sel: SelKind,
    pub sel_key: String,

    /// 镜像层数缓存（id → RootFS.Layers.len()）：列表 API 不返回层数，
    /// 首次拿到镜像列表后逐个 inspect 回填，此后仅对新增镜像补查
    pub image_layers: HashMap<String, usize>,
    /// 卷磁盘占用缓存（name → 字节）：来自低频 `/system/df`
    pub volume_sizes: HashMap<String, u64>,

    // —— Phase 3 日志面板（环形缓冲，最多 500 行）——
    pub logs: Vec<String>,
    pub logs_cap: usize,
    pub stats_prev: HashMap<String, ContainerStatsSnapshot>,

    // —— 优雅退出 —— consumer 线程定期检查这个 flag
    pub shutdown: AtomicBool,

    pub sort_field: SortField,
    pub sort_desc: bool,
    pub filter: String,
    /// 当前视图 0=容器 1=镜像 2=网络 3=卷（真值在 UI，回写 state 仅为持久化）
    pub view: i64,
    /// 主题模式 0=auto 1=light 2=dark
    pub theme: i64,
    /// 日志自动跟随
    pub log_follow: bool,
    pub batch: BatchState,
}

impl AppState {
    /// 创建 AppState，**返回 (state, rx)**；rx 交给 consumer 线程独占
    pub fn new() -> (Arc<Mutex<Self>>, mpsc::Receiver<Msg>) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        // 首次启动（无配置文件）用内置默认；之后以配置文件为准
        let (sort_field, sort_desc, filter, view, theme, log_follow, query) =
            match crate::config::AppConfig::load() {
                Some(c) => (
                    SortField::from_config_index(c.sort_field),
                    c.sort_desc,
                    c.filter,
                    c.view,
                    c.theme,
                    c.log_follow,
                    c.search,
                ),
                None => (
                    SortField::Group,
                    false,
                    "all".into(),
                    0,
                    0,
                    true,
                    String::new(),
                ),
            };

        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Self {
            rt,
            tx,
            containers: Vec::new(),
            images: Vec::new(),
            networks: Vec::new(),
            volumes: Vec::new(),
            last_refresh: Instant::now() - Duration::from_secs(10),
            container_query: query.clone(),
            image_query: query,
            detail: None,
            image_detail: None,
            sel: SelKind::Container,
            sel_key: String::new(),
            image_layers: HashMap::new(),
            volume_sizes: HashMap::new(),
            logs: Vec::new(),
            logs_cap: 500,
            stats_prev: HashMap::new(),
            shutdown: AtomicBool::new(false),
            sort_field,
            sort_desc,
            filter,
            view,
            theme,
            log_follow,
            batch: BatchState::default(),
        }));
        (state, rx)
    }

    /// 把当前 UI 状态写入配置文件（低频回调/退出时调用；小文件原子写）
    pub fn persist(&self) {
        crate::config::AppConfig {
            theme: self.theme,
            view: self.view,
            filter: self.filter.clone(),
            sort_field: self.sort_field.to_config_index(),
            sort_desc: self.sort_desc,
            search: self.container_query.clone(),
            log_follow: self.log_follow,
        }
        .save();
    }

    /// 触发四类资源列表刷新（锁内安全：只 spawn，不做 IO）
    pub fn refresh(&mut self) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let (c_res, i_res, n_res, v_res) = futures::join!(
                list_containers(true),
                list_images(),
                list_networks(),
                list_volumes(),
            );
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
            let _ = tx.send(Msg::Networks(n_res));
            let _ = tx.send(Msg::Volumes(v_res));
        });
        self.last_refresh = Instant::now();
    }

    /// 回填尚未缓存的镜像层数（锁内 spawn，单条 Msg 批量回传）
    ///
    /// inspect 失败的镜像按 0 层写入缓存，避免每 3s 反复重试。
    fn request_missing_layers(&mut self) {
        let missing: Vec<String> = self
            .images
            .iter()
            .filter(|i| !self.image_layers.contains_key(&i.id))
            .map(|i| i.id.clone())
            .collect();
        if missing.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let futs = missing
                .into_iter()
                .map(|id| async move {
                    let n = crate::docker::image_layer_count(&id).await.unwrap_or(0);
                    (id, n)
                });
            let got: Vec<(String, usize)> = futures::future::join_all(futs).await;
            let _ = tx.send(Msg::ImageLayers(got));
        });
    }

    /// 请求 inspect 一个容器（供 callbacks.rs 在锁内 spawn）
    pub fn request_detail(&mut self, id: String) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let res = inspect_container(&id).await;
            let _ = tx.send(Msg::ContainerDetail(res));
        });
    }

    /// 请求 inspect 一个镜像
    pub fn request_image_detail(&mut self, name: String) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let res = crate::docker::inspect_image(&name).await;
            let _ = tx.send(Msg::ImageDetail(res));
        });
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    /// 消费一批 Msg，返回 (need_ui_update, latest_msg) — 两阶段：先处理 first、再 try_recv 扫积压
    pub fn drain_messages(
        &mut self,
        rx: &mpsc::Receiver<Msg>,
        first: Option<Msg>,
    ) -> (bool, Option<Msg>) {
        let mut need_ui_update = false;
        let mut latest_msg: Option<Msg> = None;

        let mut handle_one = |msg: Msg| {
            // Stats：每条都写回 containers（累加）
            if let Msg::Stats(id, curr) = &msg {
                let cpu = compute_cpu_delta(self.stats_prev.get(id), curr);
                self.stats_prev.insert(id.clone(), curr.clone());
                if let Some(c) = self.containers.iter_mut().find(|c| c.id == *id) {
                    c.cpu_percent = cpu;
                    c.mem_usage = curr.mem_usage;
                    c.mem_limit = curr.mem_limit;
                    c.net_rx = curr.net_rx;
                    c.net_tx = curr.net_tx;
                }
            }
            match &msg {
                Msg::Containers(Ok(list)) => {
                    // ⚠️ 保留旧容器 stats，避免 3s 列表刷新冲掉 6s stats 写入
                    let mut old: HashMap<String, (f64, u64, u64, u64, u64)> = self
                        .containers
                        .iter()
                        .map(|c| {
                            (
                                c.id.clone(),
                                (c.cpu_percent, c.mem_usage, c.mem_limit, c.net_rx, c.net_tx),
                            )
                        })
                        .collect();
                    self.containers = list.clone();
                    for c in &mut self.containers {
                        if let Some(v) = old.remove(&c.id) {
                            (c.cpu_percent, c.mem_usage, c.mem_limit, c.net_rx, c.net_tx) = v;
                        }
                    }
                }
                Msg::Images(Ok(list)) => {
                    self.images = list.clone();
                    // 列表 API 不给层数，首次/新增镜像后台补查一次
                    self.request_missing_layers();
                }
                Msg::Networks(Ok(list)) => self.networks = list.clone(),
                Msg::Volumes(Ok(list)) => self.volumes = list.clone(),
                Msg::ImageLayers(pairs) => {
                    for (id, n) in pairs {
                        self.image_layers.insert(id.clone(), *n);
                    }
                }
                Msg::VolumeSizes(map) => self.volume_sizes = map.clone(),
                Msg::ContainerDetail(Ok(d)) => self.detail = Some(d.clone()),
                Msg::ImageDetail(Ok(d)) => self.image_detail = Some(d.clone()),
                Msg::LogLine(line) => {
                    self.logs.push(line.clone());
                    if self.logs.len() > self.logs_cap {
                        self.logs.drain(0..self.logs.len() - self.logs_cap);
                    }
                }
                _ => {}
            }
            latest_msg = Some(msg);
            need_ui_update = true;
        };

        if let Some(msg) = first {
            handle_one(msg);
        }
        while let Ok(msg) = rx.try_recv() {
            handle_one(msg);
        }
        (need_ui_update, latest_msg)
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
