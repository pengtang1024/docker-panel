//! 自动更新状态（检查结果 / 进度 / 双向 busy 标志）
//!
//! 从 `AppState` 拆出：检查与安装共用 `update_busy` 互斥，UI 进度与
//! 结果只由 `ui_update/update.rs` 与 `update/` 任务读写。拆开后更新
//! 相关字段有单一归属，不与列表 / 日志状态混在同一结构里。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::models::update::UpdateInfo;

/// 自动更新状态
#[derive(Default)]
pub struct UpdateState {
    /// 启动时检查更新（默认开）
    pub check_updates: bool,
    /// 检查到新版本后自动下载安装（默认开）
    pub auto_install: bool,
    /// 安装成功后自动重启应用（默认开）
    pub auto_restart: bool,
    /// 上次检查时间（unix 秒），24h 节流
    pub last_update_check: i64,
    /// 最近一次成功检查到的 release（含本机安装计划）
    pub update_info: Option<UpdateInfo>,
    /// 是否有比当前版本更新的 release（顶栏徽标）
    pub update_available: bool,
    /// 已安装更新，等待重启
    pub update_restart_needed: bool,
    /// 下载/安装进度 0~100
    pub update_progress: i32,
    /// 更新状态文案（检查中 / 下载中 / 错误）
    pub update_status: String,
    /// 弹窗 busy 状态（consumer 根据消息推进；与互斥用的 update_busy 分离）
    pub update_busy_ui: bool,
    /// 检查/安装互斥标志（防重复任务）
    pub update_busy: Arc<AtomicBool>,
}
