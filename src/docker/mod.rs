//! bollard 封装：所有 Docker API 调用都通过本模块发起
//!
//! 约定：
//! - 连接地址不硬编码，统一使用 `Docker::connect_with_local_defaults()`
//!   （自动读取 `DOCKER_HOST，默认本机` unix socket /var/run/docker.sock）
//! - 所有函数都是 async，由上层 tokio 运行时在后台线程执行，不阻塞 GUI

use bollard::Docker;

mod containers;
mod images;
mod system;

pub use containers::{
    list_containers, remove_container, restart_container, start_container, stop_container,
};
pub use images::{list_images, pull_image, remove_image};
pub use system::cleanup;

/// 建立本机 Docker 连接
pub fn connect() -> Result<Docker, String> {
    Docker::connect_with_local_defaults().map_err(|e| format!("连接 Docker 失败: {e}"))
}
