//! 日志流状态提示文案（i18n key 的**唯一定义处**）
//!
//! 三条提示分别对应日志流的三种结局：
//! - `NOTICE_INTERRUPTED`：连接中断，正在退避重连
//! - `NOTICE_RECOVERED`：连接已恢复
//! - `NOTICE_STOPPED`：流已结束（容器停止 / 被删除 / 致命错误）
//!
//! 其余模块只引用这些常量，不再内联字面量（#28）。
//! 提示的**清除**由 UI 侧负责：`ui_apply::clear_log_panel` / `clear_log_status`。

/// 日志流中断提示（i18n key）
pub const NOTICE_INTERRUPTED: &str = "Log stream interrupted, reconnecting…";
/// 日志流已恢复提示（i18n key）
pub const NOTICE_RECOVERED: &str = "Log stream reconnected";
/// 日志流已停止提示（i18n key）
pub const NOTICE_STOPPED: &str = "Log stream stopped";
