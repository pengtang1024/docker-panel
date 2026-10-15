//! 应用共享类型：Tab、筛选条件、行动作、弹窗状态、视图状态
//!
//! 这些类型被多个子模块（containers、images、windows、tasks）共享使用，
//! 独立出来方便引用，也让 mod.rs 保持精简。

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

use crate::models::{ChartPoint, LogEvent};

/// 顶部 Tab
#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum Tab {
    Containers,
    Images,
}

/// 容器筛选条件
#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum Filter {
    Running,
    All,
}

/// 表格行内待执行的动作（渲染结束后统一执行）
#[derive(Clone)]
pub(crate) enum RowAction {
    Logs(String, String),
    Stats(String, String),
    Start(String, String),
    Stop(String, String),
    Restart(String, String),
    DeleteContainer(String, String),
    DeleteImage(String, String),
}

/// 二次确认弹窗（变体名统一以 Delete 开头，便于一眼读出弹窗性质）
#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Confirm {
    DeleteContainer {
        id: String,
        name: String,
    },
    DeleteImage {
        name: String,
        tag: String,
    },
    /// 批量删除容器（items: (id, 名称)）
    DeleteContainers {
        items: Vec<(String, String)>,
    },
    /// 批量删除镜像（items: (名称, tag)）
    DeleteImages {
        items: Vec<(String, String)>,
    },
}

/// 日志查看窗口状态
pub(crate) struct LogView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) lines: Vec<String>,
    /// 是否跟随滚动到底部
    pub(crate) follow: bool,
    /// 日志流是否已结束
    pub(crate) ended: bool,
    pub(crate) rx: Receiver<LogEvent>,
    pub(crate) handle: Option<tokio::task::JoinHandle<()>>,
}

/// 资源监控窗口状态
pub(crate) struct ResourceView {
    pub(crate) id: String,
    pub(crate) name: String,
    /// 历史采样点（最多保留 HISTORY_CAP 个 ≈ 4 分钟；VecDeque 便于头部淘汰）
    pub(crate) history: VecDeque<ChartPoint>,
}
