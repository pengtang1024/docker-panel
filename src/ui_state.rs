//! UI 状态 + 消息处理
//!
//! 线程模型（修死锁版）：
//! - AppState（Arc<Mutex<>>）只放**可共享**的状态：tokio runtime、tx、列表数据、query
//! - mpsc::Receiver<Msg> **不在 Mutex 内** — consumer 线程独占它
//!   → consumer 的 recv() 永远不阻塞主线程拿 state 锁
//! - 详情字段格式化见 detail_fmt.rs；consumer 泵见 consumer.rs

use std::collections::{BTreeSet, HashMap};
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
    ContainerDetail, ContainerInfo, ImageInfo, Msg, NetworkInfo, ResourceSort, SortField,
    VolumeInfo,
};

/// 日志窗口裁剪 + UI 日志行模型同步账（独立模块，见 ui_state/log_sync.rs）
pub mod config_io;
pub mod follow_state;
pub mod log_sync;
pub use log_sync::{LogSyncPlan, LOG_ROWS_MAX};

/// 抽屉当前展示的资源类型（决定详情区渲染哪份数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelKind {
    #[default]
    Container,
    Image,
    Network,
    Volume,
    /// 分组合并日志：抽屉只展示日志（没有单个容器的详情页）
    Group,
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

    // —— Phase 3 日志面板（滚动窗口，最多 LOG_ROWS_MAX 行）——
    pub logs: Vec<String>,
    pub logs_cap: usize,
    /// 日志流累计进窗口的行数：给窗口在全局行流中定位（窗口 = 末尾 logs.len() 行）
    pub logs_total: u64,
    /// UI 日志行模型已同步到的累计行数
    pub model_total: u64,
    /// UI 日志行模型当前行数
    pub model_rows: usize,
    /// 上次同步时的流 epoch：epoch 变了说明模型已被清空，行数跟踪作废
    pub model_epoch: u64,
    /// 当前日志流 epoch：用于 Msg::LogLine / LogStreamEnded 的流代际过滤
    pub stream_epoch: u64,
    /// 当前选中会话是否已加载过历史日志：收到首条日志或用户 Clear 后置 true；
    /// 重新选中容器（clear_logs）重置。空历史/首流失败可再次以 tail=200 重试。
    pub history_done: bool,
    /// 日志缓冲 epoch：仅用于 UI 日志行列表的过期判断；Clear/切容器时递增
    pub log_epoch: Arc<AtomicU64>,
    /// 当前日志跟随流的幂等键：单容器 = 容器 id；分组合并日志 = 分组键
    /// （空 = 无流）；用于避免重复建流
    pub follow_id: String,
    /// 当前流的全部目标 `(容器 id, 行前缀)`：
    /// 单容器 1 项且前缀为空；分组合并日志 = 每个运行中成员一项，前缀 `[容器名]`
    pub follow_targets: Vec<(String, String)>,
    /// 日志跟随流取消信号：换容器/清空时 drop sender，旧流 select 到关闭后退出
    pub follow_cancel: Option<tokio::sync::watch::Sender<()>>,
    /// 当前日志流每条 stream 的存活标志（每条流独立）：旧流退出不会误标新流；
    /// 分组合并日志有多条流，全部退出才算流结束
    pub follow_active: Vec<Arc<AtomicBool>>,
    pub stats_prev: HashMap<String, ContainerStatsSnapshot>,

    // —— 优雅退出 —— consumer 线程定期检查这个 flag
    pub shutdown: AtomicBool,

    pub sort_field: SortField,
    pub sort_desc: bool,
    /// 行模型 epoch：排序/筛选/搜索等改变行内容的操作递增。
    ///
    /// consumer 在锁内快照行模型、锁外才写 UI；这期间用户若改了排序条件，
    /// 那批行就是按旧条件建的，落地后列表顺序会与排序箭头打架。
    /// 回调改条件时会重建行并递增 epoch，consumer 应用前比对 epoch，
    /// 不一致就丢弃这批旧行（与日志行模型的 log_epoch 同一套做法）。
    pub rows_epoch: Arc<AtomicU64>,
    pub filter: String,
    /// 当前视图 0=容器 1=镜像 2=网络 3=卷（真值在 UI，回写 state 仅为持久化）
    pub view: i64,
    /// 主题模式 0=auto 1=light 2=dark
    pub theme: i64,
    /// 日志自动跟随
    pub log_follow: bool,
    pub batch: BatchState,
    /// 台账内联树：**收起**的分组键集合
    ///
    /// 键格式：`project:<项目名>` / `svc:<项目名>/<服务名>`。
    /// 存「收起」而不是「展开」——新出现的分组默认展开（集合里没有 = 展开）。
    ///
    /// 用户操作过的状态 —— 只由 UI 回调写，consumer 只读。
    pub compose_collapsed: BTreeSet<String>,
    /// 镜像列表排序（0=名称 1=大小 2=创建时间）
    pub sort_images: ResourceSort,
    /// 网络列表排序（0=名称 1=driver 2=子网）
    pub sort_networks: ResourceSort,
    /// 卷列表排序（0=名称 1=大小 2=挂载点）
    pub sort_volumes: ResourceSort,
}

impl AppState {
    /// 创建 AppState，**返回 (state, rx)**；rx 交给 consumer 线程独占
    pub fn new() -> (Arc<Mutex<Self>>, mpsc::Receiver<Msg>) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        // 首次启动（无配置文件）用内置默认；之后以配置文件为准
        let cfg = config_io::load();
        let sort_field = cfg.sort_field;
        let sort_desc = cfg.sort_desc;
        let filter = cfg.filter;
        let view = cfg.view;
        let theme = cfg.theme;
        let log_follow = cfg.log_follow;
        let query = cfg.query.clone();
        let compose_collapsed = cfg.compose_collapsed;
        let sort_images = cfg.sort_images;
        let sort_networks = cfg.sort_networks;
        let sort_volumes = cfg.sort_volumes;

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
            logs_cap: LOG_ROWS_MAX,
            logs_total: 0,
            model_total: 0,
            model_rows: 0,
            model_epoch: 0,
            stream_epoch: 0,
            history_done: false,
            log_epoch: Arc::new(AtomicU64::new(0)),
            follow_id: String::new(),
            follow_targets: Vec::new(),
            follow_cancel: None,
            follow_active: Vec::new(),
            stats_prev: HashMap::new(),
            shutdown: AtomicBool::new(false),
            sort_field,
            sort_desc,
            rows_epoch: Arc::new(AtomicU64::new(0)),
            filter,
            view,
            theme,
            log_follow,
            batch: BatchState::default(),
            compose_collapsed,
            sort_images,
            sort_networks,
            sort_volumes,
        }));
        (state, rx)
    }

    /// 把当前 UI 状态写入配置文件（低频回调/退出时调用；小文件原子写）
    pub fn persist(&self) {
        config_io::save(self);
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
        s.follow_active = vec![Arc::new(AtomicBool::new(true))];
        assert!(s.follow_is_alive());
        // 多路合并流：只要还有一条在跑就仍然存活
        s.follow_active = vec![
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(true)),
        ];
        assert!(s.follow_is_alive());
        s.follow_active = vec![Arc::new(AtomicBool::new(false))];
        assert!(!s.follow_is_alive());
        s.follow_active.clear();
        assert!(!s.follow_is_alive());
    }
}
