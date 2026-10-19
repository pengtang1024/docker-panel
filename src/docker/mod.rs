//! bollard 封装层——所有 Docker API 调用的唯一入口
//!
//! 约定：
//! - 每次调用新建连接（bollard 连接极轻量，跨任务无状态）
//! - 所有 pub 函数都是 async，由上层 tokio runtime 在后台线程执行
//! - 错误统一转中文 `String` 上抛，UI 层直接当 status_text 用
//!
//! 分层：
//!   containers.rs  → 容器列表 + CRUD
//!   images.rs      → 镜像列表 + 拉取 + 删除（拉取含进度回调）
//!   system.rs      → prune 清理

use std::future::Future;
use std::time::Duration;

use bollard::Docker;

mod containers;
mod image_details;
mod images;
mod logs;
mod netvol;
mod stats;
mod system;

pub use containers::{
    list_containers, remove_container, restart_container, start_container, stop_container,
};
pub use image_details::inspect_image;
pub use images::{image_layer_count, list_images, pull_image, remove_image};
pub use logs::{
    follow_logs, inspect_container, retry_delay, LogError, LogStreamEnd, LogStreamOutcome,
    RecoverNotice, LOG_IDLE_TIMEOUT,
};
pub use netvol::{list_networks, list_volumes, remove_network, remove_volume, volume_sizes};
pub use stats::{compute_cpu_delta, fetch_stats, ContainerStatsSnapshot};
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

/// 建立本机 Docker 连接
pub fn connect() -> Result<Docker, String> {
    Docker::connect_with_local_defaults()
        .map_err(|e| crate::i18n::trf("Failed to connect to Docker: {0}", &[&e.to_string()]))
}
