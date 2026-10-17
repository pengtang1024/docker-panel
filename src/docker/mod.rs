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
pub use images::{list_images, pull_image, remove_image};
pub use logs::{follow_logs, inspect_container};
pub use netvol::{list_networks, list_volumes};
pub use stats::{compute_cpu_delta, fetch_stats, ContainerStatsSnapshot};
pub use system::cleanup;

/// 建立本机 Docker 连接
pub fn connect() -> Result<Docker, String> {
    Docker::connect_with_local_defaults()
        .map_err(|e| crate::i18n::trf("Failed to connect to Docker: {0}", &[&e.to_string()]))
}
