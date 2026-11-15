//! 日志面板状态（滚动窗口 + 跟随流账本）
//!
//! 从 `AppState` 拆出：日志相关的 14 个字段只在「建流 / 收行 / 清空 / 同步
//! UI 行模型」这几个场景里一起变，独立成结构后调用点只碰 `state.log`，
//! 与列表 / 排序等无关状态解耦。
//!
//! ⚠️ 字段语义与规范 #31（游标纪律）、#33（洪峰抽样）、#15（只追加不重建）
//! 对应，搬迁时不得改变任何初始值。

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use super::LOG_ROWS_MAX;

/// 日志面板状态
pub struct LogState {
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
    /// 已投递到通道、但还没被 consumer 应用的行数。
    ///
    /// 生产者（`docker::logs`）据此感知「UI 侧跟不上了」：超过阈值就按积压比例
    /// 抽样显示，避免 10 万行/秒量级把通道撑成无界队列（RSS 涨、延迟无上限）。
    pub log_backlog: Arc<AtomicU64>,
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
    pub follow_active: Vec<Arc<std::sync::atomic::AtomicBool>>,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            logs: Vec::new(),
            logs_cap: LOG_ROWS_MAX,
            logs_total: 0,
            model_total: 0,
            model_rows: 0,
            model_epoch: 0,
            stream_epoch: 0,
            history_done: false,
            log_epoch: Arc::new(AtomicU64::new(0)),
            log_backlog: Arc::new(AtomicU64::new(0)),
            follow_id: String::new(),
            follow_targets: Vec::new(),
            follow_cancel: None,
            follow_active: Vec::new(),
        }
    }
}
