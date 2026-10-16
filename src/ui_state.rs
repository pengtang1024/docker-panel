//! UI 状态 + 消息处理
//!
//! 线程模型（修死锁版）：
//! - AppState（Arc<Mutex<>>）只放**可共享**的状态：tokio runtime、tx、containers/images、query
//! - mpsc::Receiver<Msg> **不在 Mutex 内** — consumer 线程独占它
//!   → 这样 consumer 的 recv() 永远不阻塞主线程拿 state 锁
//! - 主线程 Timer/回调 → lock state → spawn refresh/task → send tx
//! - consumer 线程：阻塞 recv() → 收到消息后 lock state → drain_messages → 构建 UI rows
//!   → drop state 锁 → invoke_from_event_loop 更新 Slint Model

use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;

use crate::docker::{list_containers, list_images};
use crate::models::{ContainerInfo, ImageInfo, Msg};

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
        }));
        (state, rx)
    }

    /// 触发一次 Docker 列表刷新（在 Mutex 锁内调用，立刻释放）
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

    /// 处理消息：先处理一条刚 recv() 取到的（if any），再 try_recv 扫积压
    /// 这样 recv() 消费走的那条不会被丢
    pub fn drain_messages(&mut self, rx: &mpsc::Receiver<Msg>, first: Option<Msg>) -> (bool, Option<Msg>) {
        let mut need_ui_update = false;
        let mut latest_msg: Option<Msg> = None;
        let mut count = 0usize;

        // 1. 先处理 recv() 消费走的那条（try_recv 再也看不到它了）
        if let Some(msg) = first {
            count += 1;
            eprintln!("[drain] msg #{count}(recv): variant={:?}", std::mem::discriminant(&msg));
            Self::handle_one(msg, &mut need_ui_update, &mut latest_msg, &mut self.containers, &mut self.images);
        }
        // 2. 再扫 try_recv 积压
        while let Ok(msg) = rx.try_recv() {
            count += 1;
            eprintln!("[drain] msg #{count}(try): variant={:?}", std::mem::discriminant(&msg));
            Self::handle_one(msg, &mut need_ui_update, &mut latest_msg, &mut self.containers, &mut self.images);
        }
        eprintln!("[drain] total={count} containers={} images={} need={need_ui_update}", self.containers.len(), self.images.len());
        (need_ui_update, latest_msg)
    }

    fn handle_one(
        msg: Msg,
        need_ui_update: &mut bool,
        latest_msg: &mut Option<Msg>,
        containers: &mut Vec<ContainerInfo>,
        images: &mut Vec<ImageInfo>,
    ) {
        match &msg {
            Msg::Containers(Ok(list)) => {
                eprintln!("[drain] → Containers(Ok) len={}", list.len());
                *containers = list.clone();
                *need_ui_update = true;
                *latest_msg = Some(msg);
            }
            Msg::Containers(Err(e)) => {
                eprintln!("[drain] → Containers(Err) {e}");
                *latest_msg = Some(Msg::Containers(Err(e.clone())));
                *need_ui_update = true;
            }
            Msg::Images(Ok(list)) => {
                eprintln!("[drain] → Images(Ok) len={}", list.len());
                *images = list.clone();
                *need_ui_update = true;
                *latest_msg = Some(msg);
            }
            Msg::Images(Err(e)) => {
                eprintln!("[drain] → Images(Err) {e}");
                *latest_msg = Some(Msg::Images(Err(e.clone())));
            }
            Msg::ActionDone(_, _)
            | Msg::PullProgress(_)
            | Msg::PullDone(_)
            | Msg::CleanupDone(_) => {
                *latest_msg = Some(msg);
                *need_ui_update = true;
            }
        }
    }
}

/// UI 更新意图（从最新消息推导，由 main 负责应用）
pub struct UiUpdate {
    pub docker_ok: bool,
    pub status_text: String,
    pub close_pull: bool,
    pub reset_pull_busy: bool,
}

pub fn derive_update(containers_len: usize, latest_msg: &Option<Msg>) -> UiUpdate {
    match latest_msg {
        Some(Msg::Containers(Ok(_))) => UiUpdate {
            docker_ok: true,
            status_text: format!("已加载 {containers_len} 个容器"),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::Containers(Err(e))) => UiUpdate {
            docker_ok: false,
            status_text: format!("容器刷新失败: {e}"),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::Images(Err(e))) => UiUpdate {
            docker_ok: false,
            status_text: format!("镜像刷新失败: {e}"),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::ActionDone(desc, Ok(_))) => UiUpdate {
            docker_ok: true,
            status_text: desc.clone(),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::ActionDone(desc, Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: format!("{desc}: {e}"),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::PullProgress(t)) => UiUpdate {
            docker_ok: true,
            status_text: t.clone(),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::PullDone(Ok(succ))) => UiUpdate {
            docker_ok: true,
            status_text: succ.clone(),
            close_pull: true,
            reset_pull_busy: true,
        },
        Some(Msg::PullDone(Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: format!("拉取失败: {e}"),
            close_pull: false,
            reset_pull_busy: true,
        },
        Some(Msg::CleanupDone(Ok(succ))) => UiUpdate {
            docker_ok: true,
            status_text: succ.clone(),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::CleanupDone(Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: format!("清理失败: {e}"),
            close_pull: false,
            reset_pull_busy: false,
        },
        _ => UiUpdate {
            docker_ok: true,
            status_text: "就绪".into(),
            close_pull: false,
            reset_pull_busy: false,
        },
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
