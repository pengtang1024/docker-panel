//! bollard 封装层——所有 Docker API 调用的唯一入口
//!
//! 约定：
//! - **共享单例客户端**：`Docker` 内部是 `Arc<Transport>`（hyper 连接池），
//!   clone 极廉价且共享池 → 全进程只 `connect_with_local_defaults` 一次，
//!   之后每次调用都拿缓存实例的 clone，hyper 池跨调用复用 Unix socket 连接。
//!   此前「每次调用新建连接」会让每 3s 刷新建 4 个客户端、每 6s stats 每容器
//!   再建 1 个（11 容器 = 每分钟 ~110 个），且用完即弃、连接池零复用。
//! - 所有 pub 函数都是 async，由上层 tokio runtime 在后台线程执行
//! - 错误统一转中文 `String` 上抛，UI 层直接当 status_text 用
//!
//! 分层：
//!   containers.rs     → 容器列表 + CRUD
//!   container_detail.rs → 容器详情 inspect + 存活性探针 probe
//!   images.rs         → 镜像列表 + 拉取 + 删除（拉取含进度回调）
//!   log_types.rs      → 日志流类型 / 错误分类 / 退避策略（纯类型）
//!   logs.rs           → 日志流读取（把 Docker 日志搬成 Msg）
//!   system.rs         → prune 清理
//!
//! ⚠️ 性能契约（R10 惯例）：`connect()` 是进程级单例（OnceLock + clone），
//! 禁止在调用点自行 `connect_with_local_defaults()` 新建客户端——那样会
//! 绕开共享连接池，每次调用都重连 Unix socket。

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use bollard::Docker;

pub(crate) mod container_detail;
mod containers;
mod image_details;
mod images;
pub(crate) mod log_types;
mod logs;
mod netvol;
pub(crate) mod raw_http;
mod stats;
mod system;

pub use container_detail::{inspect_container, probe_container, ContainerReach};
pub use containers::{
    list_containers, remove_container, restart_container, start_container, stop_container,
};
pub use image_details::inspect_image;
pub use images::{image_layer_count, list_images, pull_image, remove_image};
pub use log_types::{
    retry_delay, LogStreamEnd, IDLE_RETRY_DELAY, KEEPALIVE_INTERVAL, LOG_IDLE_TIMEOUT,
};
pub use logs::follow_logs;
pub use netvol::{list_networks, list_volumes, remove_network, remove_volume, volume_sizes};
pub use stats::{compute_cpu_delta, sample_all, ContainerStatsSnapshot};
pub use system::cleanup;

/// 单次 Docker API 调用上限：守护进程无响应时不能让后台任务无限挂起，
/// 否则 3s/6s interval 会反复叠加任务，最终把 tokio 工作线程耗尽
///（表现为列表/日志停止更新，看起来像"卡死"）。
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// 给任意 Docker 调用套统一超时（仅用于可重试的列表/采样/inspect）。
pub async fn with_timeout<T>(fut: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    match tokio::time::timeout(CALL_TIMEOUT, fut).await {
        Ok(res) => res,
        Err(_) => Err(crate::i18n::t("Docker API timeout")),
    }
}

/// 共享 Docker 客户端（进程级单例）。
///
/// `Docker` 是 `Arc<Transport>` 包裹的 hyper 客户端：clone 只复制 Arc，
/// 连接池由所有 clone 共享 → 列表刷新 / stats 采样 / inspect 复用同一批
/// Unix socket 连接，不再每次调用都重连。
static DOCKER: OnceLock<Result<Docker, String>> = OnceLock::new();

/// 建立本机 Docker 连接（首次调用建客户端，之后返回共享实例的 clone）。
pub fn connect() -> Result<Docker, String> {
    match DOCKER.get_or_init(|| {
        Docker::connect_with_local_defaults()
            .map_err(|e| crate::i18n::trf("Failed to connect to Docker: {0}", &[&e.to_string()]))
    }) {
        Ok(d) => Ok(d.clone()),
        Err(e) => Err(e.clone()),
    }
}
