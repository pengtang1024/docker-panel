//! 全局策略常量：刷新/采样周期、历史容量、窗口稳定 ID
//!
//! 集中管理，避免魔法数字与窗口 ID 字符串散落在各渲染模块。
//! 所有常量仅在 `crate::app` 内可见，子模块经 `use super::*` 继承。

// ---------- 定时刷新与采样 ----------

/// 容器列表自动刷新周期（秒）
pub(in crate::app) const CONTAINER_REFRESH_SECS: u64 = 2;
/// 镜像列表自动刷新周期（秒）
pub(in crate::app) const IMAGE_REFRESH_SECS: u64 = 15;
/// 资源采样周期（秒）
pub(in crate::app) const STATS_SAMPLE_SECS: u64 = 2;
/// 资源采样并发上限
pub(in crate::app) const STATS_CONCURRENCY: usize = 4;
/// 启动时把「上次刷新时刻」提前的秒数，让首屏数据尽快拉取
pub(in crate::app) const INITIAL_REFRESH_LEAD_SECS: u64 = 10;
/// 帧重绘节流间隔（毫秒），保证后台 channel 消息被及时处理
pub(in crate::app) const REPAINT_INTERVAL_MS: u64 = 250;

// ---------- 历史数据容量 ----------

/// 资源监控曲线最多保留的采样点数（120 × 2s ≈ 4 分钟）
pub(in crate::app) const HISTORY_CAP: usize = 120;
/// 拉取进度文本保留上限，超出后丢弃最旧的 `PULL_PROGRESS_DROP` 行
pub(in crate::app) const PULL_PROGRESS_CAP: usize = 200;
pub(in crate::app) const PULL_PROGRESS_DROP: usize = 100;
/// 日志窗口行数保留上限，超出后丢弃最旧的 `LOG_LINES_DROP` 行
pub(in crate::app) const LOG_LINES_CAP: usize = 5000;
pub(in crate::app) const LOG_LINES_DROP: usize = 500;
/// 打开日志窗口时向 Docker 回溯的历史行数
pub(in crate::app) const LOG_TAIL_LINES: usize = 200;

// ---------- egui 窗口稳定 ID（跨帧保持窗口状态，集中管理避免拼错） ----------

pub(in crate::app) const WIN_CONFIRM: &str = "docker_panel_confirm_window";
pub(in crate::app) const WIN_LOG: &str = "docker_panel_log_window";
pub(in crate::app) const WIN_RESOURCE: &str = "docker_panel_resource_window";
pub(in crate::app) const WIN_CLEANUP: &str = "docker_panel_cleanup_window";
pub(in crate::app) const WIN_ERROR: &str = "docker_panel_error_window";
pub(in crate::app) const WIN_RESULT: &str = "docker_panel_result_window";
