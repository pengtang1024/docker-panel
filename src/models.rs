//! 数据结构体 + channel 消息枚举
//!
//! Msg 枚举是三线程架构里唯一跨线程传递的数据类型：
//!   tokio spawn 的异步任务 → tx.send(Msg) → mpsc channel →
//!   consumer 线程 recv()/try_recv() → drain_messages 更新 AppState →
//!   consumer drop 锁 → invoke_from_event_loop 更新 Slint Model
//!
//! 各变体发送者一览：
//!   Containers/Images     ← ui_state::refresh() spawn 的列表任务
//!                          main.rs 初始加载 spawn 的列表任务
//!   PullProgress          ← docker::pull_image 的 on_progress 闭包
//!   PullDone              ← docker::pull_image 结束时 tx.send
//!   ActionDone            ← callbacks.rs 所有操作回调 spawn 的任务
//!   CleanupDone           ← callbacks.rs cleanup_submit spawn 的任务

/// 容器信息（表格一行的数据）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContainerInfo {
    /// 完整容器 ID
    pub id: String,
    /// 容器名称（去掉开头的 /）
    pub name: String,
    /// 使用的镜像名称（含 tag）
    pub image: String,
    /// 使用的镜像 ID（sha256:...，用于统计镜像关联容器数）
    pub image_id: String,
    /// 运行状态：running / exited / paused ...
    pub state: String,
    /// Docker 原始状态描述（如 "Up 2 hours"）
    pub status: String,
    /// 创建时间（unix 秒）
    pub created: i64,
    /// 端口映射文本
    pub ports: String,
    /// CPU 使用率（%）
    pub cpu_percent: f64,
    /// 内存使用量（字节）
    pub mem_usage: u64,
    /// 内存上限（字节）
    pub mem_limit: u64,
    /// 磁盘读（字节）
    pub block_read: u64,
    /// 磁盘写（字节）
    pub block_write: u64,
    /// 容器可写层占用（字节，来自 `list_containers` sizeRw）
    pub size_rw: u64,
    /// 容器根文件系统总大小（字节，含镜像只读层，来自 sizeRootFs）
    pub size_root_fs: u64,
    /// 网络接收（字节）
    pub net_rx: u64,
    /// 网络发送（字节）
    pub net_tx: u64,
    /// compose 项目名（label `com.docker.compose.project`，非 compose 启动为 None）
    pub compose_project: Option<String>,
    /// compose 服务名（label `com.docker.compose.service`）
    pub compose_service: Option<String>,
    /// 容器连接的网络名列表（从 ContainerSummary.NetworkSettings.Networks 提取）
    pub networks: Vec<String>,
}

#[allow(dead_code)]
impl ContainerInfo {
    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }

    /// 短 ID（前 12 位）
    pub fn short_id(&self) -> &str {
        let len = self.id.len().min(12);
        &self.id[..len]
    }

    /// 内存使用率（%）
    pub fn mem_percent(&self) -> f64 {
        if self.mem_limit == 0 {
            0.0
        } else {
            self.mem_usage as f64 / self.mem_limit as f64 * 100.0
        }
    }
}

/// 镜像信息（表格一行的数据）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImageInfo {
    /// 镜像 ID
    pub id: String,
    /// 镜像仓库名（无 tag 时为 `<none>`）
    pub name: String,
    /// Tag（无 tag 时为 `<none>`）
    pub tag: String,
    /// 镜像大小（字节）
    pub size: i64,
    /// 创建时间（unix 秒）
    pub created: i64,
    /// 关联容器数量
    pub containers: i64,
    /// 是否为悬空镜像（dangling）
    pub dangling: bool,
}

impl ImageInfo {
    /// 短 ID（前 12 位）
    pub fn short_id(&self) -> &str {
        let len = self.id.len().min(12);
        &self.id[..len]
    }

    /// 选中集合里的唯一键（"名称:tag"），选中/过滤/剪除选择统一走这里
    pub fn key(&self) -> String {
        format!("{}:{}", self.name, self.tag)
    }
}

/// 后台任务回传给 GUI 的消息
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Msg {
    /// 容器列表刷新结果
    Containers(Result<Vec<ContainerInfo>, String>),
    /// 镜像列表刷新结果
    Images(Result<Vec<ImageInfo>, String>),
    /// 拉取镜像进度文本
    PullProgress(String),
    /// 拉取镜像完成
    PullDone(Result<String, String>),
    /// 启动/停止/重启/删除等操作完成（操作描述, 结果）
    ActionDone(String, Result<String, String>),
    /// 清理完成
    CleanupDone(Result<String, String>),
    /// 容器 inspect 结果（详情面板）
    ContainerDetail(Result<ContainerDetail, String>),
    /// 日志流的一行（详情面板日志页签）
    LogLine(String),
    /// 单个容器的 stats 采样（delta 计算在 consumer 线程完成）
    Stats(String, crate::docker::ContainerStatsSnapshot),
    /// 网络列表
    Networks(Result<Vec<NetworkInfo>, String>),
    /// 卷列表
    Volumes(Result<Vec<VolumeInfo>, String>),
    /// 镜像 inspect 结果（镜像详情面板）
    ImageDetail(Result<ImageDetail, String>),
}

/// inspect_container 结果的精简版本——只取 UI 详情面板需要的字段
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ContainerDetail {
    pub id: String,
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub created: String,
    pub state: String,
    pub status: String,
    pub pid: i64,
    pub exit_code: i64,
    pub started_at: String,
    pub finished_at: String,
    pub ports: Vec<String>,
    pub mounts: Vec<String>,
    pub env: Vec<String>,
    pub labels: Vec<(String, String)>,
    pub command: String,
    pub working_dir: String,
    pub entrypoint: String,
    pub restart_policy: String,
    pub log_path: String,
    pub network_mode: String,
    pub networks: Vec<String>,
}

/// 网络列表简化视图（网络 Tab 用）
#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

/// 卷列表简化视图（卷 Tab 用）
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub size: String,
}

/// inspect_image 结果的精简版本——只取 UI 镜像详情面板需要的字段
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ImageDetail {
    pub id: String,
    pub short_id: String,
    pub repo_tags: String, // join(", ") 后的 "name:tag, name2:tag2"
    pub repo_digests: String,
    pub created: String,
    pub author: String,
    pub size: String, // 格式化后的大小
    pub os: String,
    pub architecture: String,
    pub entrypoint: String,
    pub cmd: String,
    pub working_dir: String,
    pub labels: String,        // join("\n") 多行 key=value
    pub env: String,           // join("\n") 多行 KEY=VALUE
    pub exposed_ports: String, // join(", ")
    pub volumes: String,       // join(", ")
}

/// 容器列表排序字段——点击表头切换
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortField {
    #[default]
    /// 按 compose_project → compose_service 分组排序（默认）
    Group,
    /// 按名称
    Name,
    /// 按运行状态（running 在前）
    Status,
    /// 按 CPU 使用率
    Cpu,
    /// 按内存使用
    Mem,
}
