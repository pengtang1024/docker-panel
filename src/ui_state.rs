//! UI 状态 + 消息处理
//!
//! 线程模型（修死锁版）：
//! - AppState（Arc<Mutex<>>）只放**可共享**的状态：tokio runtime、tx、containers/images、query
//! - mpsc::Receiver<Msg> **不在 Mutex 内** — consumer 线程独占它
//!   → 这样 consumer 的 recv() 永远不阻塞主线程拿 state 锁
//! - 主线程 Timer/回调 → lock state → spawn refresh/task → send tx
//! - consumer 线程：阻塞 recv() → 收到消息后 lock state → drain_messages → 构建 UI rows
//!   → drop state 锁 → invoke_from_event_loop 更新 Slint Model

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;

use crate::docker::{inspect_container, list_containers, list_images};
use crate::models::{ContainerDetail, ContainerInfo, ImageInfo, Msg};
use crate::slint_gen::MainWindow;
use slint::SharedString;

/// 详情面板 17 个字段的 SharedString 元组
pub type DetailTuple = (
    SharedString, SharedString, SharedString, SharedString, SharedString,
    SharedString, SharedString, SharedString, SharedString, SharedString,
    SharedString, SharedString, SharedString, SharedString, SharedString,
    SharedString, SharedString,
);

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

// —— 格式化工具 ——
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// 后台可共享状态
/// 注意：mpsc::Receiver<Msg> 不在这里 — 它由 consumer 线程独占
pub struct AppState {
    pub rt: Runtime,
    pub tx: mpsc::Sender<Msg>,

    pub containers: Vec<ContainerInfo>,
    pub images: Vec<ImageInfo>,

    pub last_refresh: Instant,
    pub container_query: String,
    pub image_query: String,

    // —— Phase 2 详情面板 ——
    pub detail: Option<ContainerDetail>,

    // —— Phase 3 日志面板（环形缓冲，最多 500 行）——
    pub logs: Vec<String>,
    pub logs_cap: usize,
}

impl AppState {
    /// 创建 AppState，**返回 (state, rx)**
    /// rx 交给 consumer 线程独占，不进 Mutex
    ///
    /// 内部会 spawn 一个后台线程永久 poll tokio runtime —
    /// 否则 current_thread runtime 里 spawn 的异步任务永远没人执行。
    pub fn new() -> (Arc<Mutex<Self>>, mpsc::Receiver<Msg>) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Self {
            rt,
            tx,
            containers: Vec::new(),
            images: Vec::new(),
            last_refresh: Instant::now() - Duration::from_secs(10),
            container_query: String::new(),
            image_query: String::new(),
            detail: None,
            logs: Vec::new(),
            logs_cap: 500,
        }));
        (state, rx)
    }

    /// 触发一次 Docker 列表刷新（在 Mutex 锁内调用，立刻释放）
    ///
    /// 为什么锁内调用安全：
    ///   refresh() 不做任何 IO——只是 spawn 两个 tokio 异步任务就返回了。
    ///   spawn 出来的任务会在**锁外**执行，真正的 Docker API 调用不会卡住主线程。
    pub fn refresh(&mut self) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let c_fut = list_containers(true);
            let i_fut = list_images();
            let (c_res, i_res) = futures::join!(c_fut, i_fut);
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
        });
        self.last_refresh = Instant::now();
    }

    /// 请求 inspect 一个容器（供 callbacks.rs 在锁内 spawn）
    pub fn request_detail(&mut self, id: String) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let res = inspect_container(&id).await;
            let _ = tx.send(Msg::ContainerDetail(res));
        });
    }

    /// 锁内调用：把 detail 格式化 17 个 SharedString
    pub fn build_detail_data(&self) -> Option<DetailTuple> {
        let d = self.detail.as_ref()?;
        let fmt = |list: &[String]| -> String {
            if list.is_empty() {
                "—".into()
            } else {
                list.join(", ")
            }
        };
        Some((
            SharedString::from(d.name.clone()),
            SharedString::from(d.state.clone()),
            SharedString::from(d.status.clone()),
            SharedString::from(d.image.clone()),
            SharedString::from(d.id.clone()),
            SharedString::from(d.pid.to_string()),
            SharedString::from(d.exit_code.to_string()),
            SharedString::from(d.created.clone()),
            SharedString::from(d.started_at.clone()),
            SharedString::from(d.restart_policy.clone()),
            SharedString::from(d.network_mode.clone()),
            SharedString::from(fmt(&d.ports)),
            SharedString::from(fmt(&d.mounts)),
            SharedString::from(fmt(&d.networks)),
            SharedString::from(d.entrypoint.clone()),
            SharedString::from(d.command.clone()),
            SharedString::from(d.working_dir.clone()),
        ))
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    /// 消费一批 Msg，更新 containers / images 并返回 (need_ui_update, latest_msg)
    ///
    /// 两阶段设计：
    ///   1. 先处理 `first` —— consumer.recv() 消费掉的那一条。
    ///      必须立即交给 drain，因为 recv() 之后它就不属于 channel 了，
    ///      try_recv() 再也看不到它。丢了 = 操作结果不回显。
    ///   2. 再 try_recv 扫 channel 里剩余的积压消息。
    ///      多条消息按 FIFO 顺序依次 handle_one。
    ///
    /// 返回的 (need_ui_update, latest_msg) 交给 main.rs 的 pump_and_apply：
    ///   need_ui_update = false → 直接 return，跳过 build rows 和 invoke
    ///   latest_msg      → derive_update() 用它推导 UI 最新意图（状态文案、docker_ok 角标等）
    pub fn drain_messages(
        &mut self,
        rx: &mpsc::Receiver<Msg>,
        first: Option<Msg>,
    ) -> (bool, Option<Msg>) {
        let mut need_ui_update = false;
        let mut latest_msg: Option<Msg> = None;

        if let Some(msg) = first {
            Self::handle_one(
                msg,
                &mut need_ui_update,
                &mut latest_msg,
                &mut self.containers,
                &mut self.images,
                &mut self.detail,
                &mut self.logs,
                self.logs_cap,
            );
        }
        while let Ok(msg) = rx.try_recv() {
            Self::handle_one(
                msg,
                &mut need_ui_update,
                &mut latest_msg,
                &mut self.containers,
                &mut self.images,
                &mut self.detail,
                &mut self.logs,
                self.logs_cap,
            );
        }
        (need_ui_update, latest_msg)
    }

    /// 单条消息处理逻辑——从 drain_messages 抽出来的纯函数化小步骤
    ///
    /// 抽出去的好处：drain_messages 只管"从哪拿消息"，
    /// handle_one 只管"拿到一条怎么改状态"。以后加新 Msg 变体只改这里。
    fn handle_one(
        msg: Msg,
        need_ui_update: &mut bool,
        latest_msg: &mut Option<Msg>,
        containers: &mut Vec<ContainerInfo>,
        images: &mut Vec<ImageInfo>,
        detail: &mut Option<ContainerDetail>,
        logs: &mut Vec<String>,
        logs_cap: usize,
    ) {
        match &msg {
            Msg::Containers(Ok(list)) => {
                *containers = list.clone();
                *need_ui_update = true;
                *latest_msg = Some(msg);
            }
            Msg::Containers(Err(e)) => {
                *latest_msg = Some(Msg::Containers(Err(e.clone())));
                *need_ui_update = true;
            }
            Msg::Images(Ok(list)) => {
                *images = list.clone();
                *need_ui_update = true;
                *latest_msg = Some(msg);
            }
            Msg::Images(Err(e)) => {
                *latest_msg = Some(Msg::Images(Err(e.clone())));
            }
            Msg::ActionDone(_, _)
            | Msg::PullProgress(_)
            | Msg::PullDone(_)
            | Msg::CleanupDone(_) => {
                *latest_msg = Some(msg);
                *need_ui_update = true;
            }
            Msg::ContainerDetail(res) => {
                if let Ok(d) = res {
                    *detail = Some(d.clone());
                }
                *latest_msg = Some(msg);
                *need_ui_update = true;
            }
            Msg::LogLine(line) => {
                logs.push(line.clone());
                if logs.len() > logs_cap {
                    let excess = logs.len() - logs_cap;
                    logs.drain(0..excess);
                }
                // 日志高频到达但只在有窗口闲置时才做 UI 更新
                *need_ui_update = true;
            }
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
