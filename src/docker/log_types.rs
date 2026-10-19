//! 日志流类型与退避策略（纯类型 + 纯函数，不触碰 Docker 连接）
//!
//! 从 `logs.rs` 拆出（R2：单文件正文 ≤300 行）：
//! 这里只放**结束原因、错误分类、退避常量**，单测可直接断言，
//! 不需要真实 Docker 守护进程。

use std::time::Duration;

/// 日志流错误（保留 Docker HTTP 状态码，便于判定不可恢复错误）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogError {
    /// 错误文本
    pub message: String,
    /// Docker HTTP 状态码（服务端错误才有；连接类错误为 None）
    pub status_code: Option<u16>,
}

impl LogError {
    /// 从 bollard 错误提取状态码（服务端错误带 404/5xx，其余仅文本）
    pub fn from_bollard(e: &bollard::errors::Error) -> Self {
        match e {
            bollard::errors::Error::DockerResponseServerError {
                status_code,
                message,
            } => LogError {
                message: message.clone(),
                status_code: Some(*status_code),
            },
            _ => LogError {
                message: e.to_string(),
                status_code: None,
            },
        }
    }

    /// 不可恢复错误：容器已被删除（HTTP 404 / No such container）
    pub fn is_fatal(&self) -> bool {
        self.status_code == Some(404)
            || self
                .message
                .to_ascii_lowercase()
                .contains("no such container")
    }
}

/// 日志流结束原因（决定后续处置）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogStreamEnd {
    /// 被取消（切容器 / Clear 重建流）
    Cancelled,
    /// 流自然结束（容器停止 / 日志 EOF）
    Eof,
    /// 长时间无任何日志行：连接可能已挂死
    Idle,
    /// Docker 侧错误：可退避重连（是否致命见 `LogError::is_fatal`）
    Error(LogError),
}

/// 日志流一次连接的结果
#[derive(Debug, Clone)]
pub struct LogStreamOutcome {
    /// 结束原因
    pub end: LogStreamEnd,
    /// 本次连接推送出的日志行数
    pub lines: u64,
    /// 本次连接是否已提示「已恢复连接」（收到首行日志时置位）
    pub notified_recovered: bool,
}

/// 日志流空闲超时：多久没有新日志就认为连接挂死（主动断流走重连路径）
pub const LOG_IDLE_TIMEOUT: Duration = Duration::from_secs(180);

/// 空闲超时后的重连延迟：连接多半只是挂死，1s 后快速重建
pub const IDLE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// 连续失败预算耗尽后的低频保活间隔：不再放弃跟随，只降频重试
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// 重连退避序列：1s → 2s → 4s → 8s → 10s（上限）
pub fn retry_delay(attempt: u32) -> Duration {
    const MAX: Duration = Duration::from_secs(10);
    if attempt >= 4 {
        return MAX;
    }
    Duration::from_secs(1u64 << attempt).min(MAX)
}
