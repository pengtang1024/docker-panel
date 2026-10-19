//! UI 状态 + 消息处理
//!
//! 线程模型（修死锁版）：
//! - AppState（Arc<Mutex<>>）只放**可共享**的状态：tokio runtime、tx、列表数据、query
//! - mpsc::Receiver<Msg> **不在 Mutex 内** — consumer 线程独占它
//!   → consumer 的 recv() 永远不阻塞主线程拿 state 锁
//! - 详情字段格式化见 detail_fmt.rs；consumer 泵见 consumer.rs

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Runtime;

use crate::batch::BatchState;
use crate::docker::{
    inspect_container, list_containers, list_images, list_networks, list_volumes,
    ContainerStatsSnapshot,
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

    /// 3s 刷新是否已有任务在跑：Docker 慢时避免每 3s 叠加一批新任务
    pub refresh_inflight: Arc<AtomicBool>,
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
    /// 当前日志流 epoch：用于 Msg::LogLine / LogStreamEnded 的流代际过滤
    pub stream_epoch: u64,
    /// 当前选中会话是否已加载过历史日志：收到首条日志或用户 Clear 后置 true；
    /// 重新选中容器（clear_logs）重置。空历史/首流失败可再次以 tail=200 重试。
    pub history_done: bool,
    /// 日志缓冲 epoch：仅用于 UI log_text 的过期判断；Clear/切容器时递增
    pub log_epoch: Arc<AtomicU64>,
    /// 当前已建立日志跟随流的容器 id（空 = 无）；用于避免重复建流
    pub follow_id: String,
    /// 日志跟随流取消信号：换容器/清空时 drop sender，旧流 select 到关闭后退出
    pub follow_cancel: Option<tokio::sync::watch::Sender<()>>,
    /// 当前日志流存活标志（每条流独立）：旧流退出不会误标新流
    pub follow_active: Option<Arc<AtomicBool>>,
    /// 被跟随容器是否仍在运行（随列表刷新更新）：运行中却 EOF 时按可恢复处理
    pub follow_running: Arc<AtomicBool>,
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
            refresh_inflight: Arc::new(AtomicBool::new(false)),
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
            stream_epoch: 0,
            history_done: false,
            log_epoch: Arc::new(AtomicU64::new(0)),
            follow_id: String::new(),
            follow_cancel: None,
            follow_active: None,
            follow_running: Arc::new(AtomicBool::new(false)),
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

    /// 触发四类资源列表刷新（锁内安全：只 spawn，不做 IO）。
    ///
    /// 上一次刷新未完成时直接跳过：Docker 慢/无响应时，3s interval 不能
    /// 每 tick 再叠 4 个调用（会耗尽 tokio 工作线程，数据看起来"冻住"）。
    pub fn refresh(&mut self) {
        if self.refresh_inflight.swap(true, Ordering::AcqRel) {
            return;
        }
        let tx = self.tx.clone();
        let inflight = self.refresh_inflight.clone();
        self.rt.spawn(async move {
            let (c_res, i_res, n_res, v_res) = futures::join!(
                crate::docker::with_timeout(list_containers(true)),
                crate::docker::with_timeout(list_images()),
                crate::docker::with_timeout(list_networks()),
                crate::docker::with_timeout(list_volumes()),
            );
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
            let _ = tx.send(Msg::Networks(n_res));
            let _ = tx.send(Msg::Volumes(v_res));
            inflight.store(false, Ordering::Release);
        });
    }

    /// 回填尚未缓存的镜像层数（锁内 spawn，单条 Msg 批量回传）
    ///
    /// inspect 失败的镜像按 0 层写入缓存，避免每 3s 反复重试。
    pub(crate) fn request_missing_layers(&mut self) {
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
            let futs = missing.into_iter().map(|id| async move {
                let n = crate::docker::with_timeout(crate::docker::image_layer_count(&id))
                    .await
                    .unwrap_or(0);
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
            let res = crate::docker::with_timeout(inspect_container(&id)).await;
            let _ = tx.send(Msg::ContainerDetail(res));
        });
    }

    /// 请求 inspect 一个镜像
    pub fn request_image_detail(&mut self, name: String) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let res = crate::docker::with_timeout(crate::docker::inspect_image(&name)).await;
            let _ = tx.send(Msg::ImageDetail(res));
        });
    }

    /// 切换选中资源：清空日志缓冲，取消旧流，并让队列里旧 epoch 的日志失效。
    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.follow_id.clear();
        self.history_done = false;
        self.log_epoch.fetch_add(1, Ordering::AcqRel);
        self.stream_epoch = self.stream_epoch.wrapping_add(1);
        // drop sender → 旧 receiver 的 changed() 立即返回，follow 任务退出
        self.follow_cancel = None;
        self.follow_active = None;
        self.follow_running.store(false, Ordering::Release);
    }

    /// Clear 工具条：清空缓冲后以 tail=0 重启当前流，只保留之后的新日志。
    ///
    /// 不能只 clear 缓冲：旧流在 mpsc 队列中已排队的行会在下一批 drain 时重新出现。
    pub fn clear_log_buffer(&mut self) {
        let had_logs = !self.logs.is_empty();
        let active = !self.follow_id.is_empty();
        self.logs.clear();
        // 只有确实清掉了历史，或当前流仍在（Clear 会丢弃其排队行）时，才标记历史已完成。
        // 空缓冲 + 无活动流时保留原标志，让下次 ensure_follow 仍可 tail=200 拉启动日志。
        if had_logs || active {
            self.history_done = true;
        }
        // 即使当前没有活动流（流已自然结束），也必须推进 epoch，
        // 让已经投递但尚未执行的旧 log_text invoke 失效。
        self.log_epoch.fetch_add(1, Ordering::AcqRel);
        if !active {
            self.follow_active = None;
            return;
        }
        let id = self.follow_id.clone();
        self.spawn_follow(&id, 0);
    }

    /// 确保对指定容器建立日志跟随流（幂等：同一容器只建一条，避免重复行）
    pub fn ensure_follow(&mut self, id: &str) {
        // 防御调用方绕过 clear_logs 直接切到另一个容器：先清掉旧缓冲，避免串台
        if !self.follow_id.is_empty() && self.follow_id != id {
            self.clear_logs();
        }
        if self.follow_id == id && self.follow_is_alive() {
            return;
        }
        // 现场计算被跟随容器是否在运行：列表刷新最多滞后 3s，
        // 直接用最新列表判断可避免「已停止容器被误判为可恢复」而空转重连
        let running = self.containers.iter().any(|c| c.id == id && c.is_running());
        self.follow_running.store(running, Ordering::Release);
        // 历史已加载过（含 Clear 后）只接续新行；否则首次拉最近 200 行。
        // history_done 在收到首条日志或 Clear 时才置位，空历史/首流失败可重试。
        let tail = crate::follow_runtime::follow_tail(self.history_done);
        self.spawn_follow(id, tail);
    }

    /// 当前日志流是否仍在运行：每条流独立标志，旧流退出不会误标新流。
    ///
    /// 用于消除「Ended 消息已入队但 consumer 尚未 drain」窗口内 ensure_follow
    /// 误判幂等、导致本次切 Tab 不建流的问题。
    fn follow_is_alive(&self) -> bool {
        self.follow_active
            .as_ref()
            .map(|a| a.load(Ordering::Acquire))
            .unwrap_or(false)
    }

    /// 建流并递增 stream epoch；不推进 buffer epoch，续流时保留已缓冲日志与待应用文本。
    ///
    /// 异步运行时（退避重连、存活标志、结束通知）全部在 `follow_runtime` 模块，
    /// 本方法只负责写入状态字段。
    fn spawn_follow(&mut self, id: &str, tail: i64) {
        self.follow_id = id.to_string();
        self.stream_epoch = self.stream_epoch.wrapping_add(1);
        let epoch = self.stream_epoch;
        let handle = crate::follow_runtime::spawn_follow(
            &self.rt,
            id.to_string(),
            tail,
            epoch,
            self.follow_running.clone(),
            self.tx.clone(),
        );
        self.follow_cancel = Some(handle.cancel);
        self.follow_active = Some(handle.active);
    }

    /// 清理弹窗各类候选数量（按当前列表估算，供弹窗 .cnt 展示）
    pub fn cleanup_counts(&self) -> (usize, usize, usize, usize) {
        let stopped = self.containers.iter().filter(|c| !c.is_running()).count();
        let dangling = self.images.iter().filter(|i| i.dangling).count();
        // 网络：bollard 不返回连接数，由容器列表反推（与卡片计数同源）
        let counts = crate::detail_fmt::network_counts(self);
        let unused_net = self
            .networks
            .iter()
            .filter(|n| !n.builtin && *counts.get(n.name.as_str()).unwrap_or(&0) == 0)
            .count();
        // 卷：无引用信息，用"匿名卷"近似可清理候选
        let anonymous_vol = self.volumes.iter().filter(|v| v.anonymous).count();
        (stopped, dangling, unused_net, anonymous_vol)
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_alive_flag_controls_idempotence() {
        let (state, _rx) = AppState::new();
        let mut s = state.lock().unwrap();
        assert!(!s.follow_is_alive());
        s.follow_active = Some(Arc::new(AtomicBool::new(true)));
        assert!(s.follow_is_alive());
        s.follow_active = Some(Arc::new(AtomicBool::new(false)));
        assert!(!s.follow_is_alive());
        s.follow_active = None;
        assert!(!s.follow_is_alive());
    }
}
