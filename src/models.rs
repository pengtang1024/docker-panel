//! 数据结构体定义：容器、镜像、资源采样点等

/// 容器信息（表格一行的数据）
#[derive(Debug, Clone)]
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
    /// 容器可写层占用（字节，来自 list_containers sizeRw）
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
}

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

/// 资源历史采样点（用于折线小图）
#[derive(Debug, Clone, Copy)]
pub struct ChartPoint {
    /// CPU 使用率（%）
    pub cpu: f64,
    /// 内存使用量（字节）
    pub mem_usage: u64,
    /// 内存上限（字节）
    pub mem_limit: u64,
}

/// 日志流事件
#[derive(Debug, Clone)]
pub enum LogEvent {
    /// 一行日志文本
    Line(String),
    /// 日志流结束
    Closed,
}

/// 后台任务回传给 GUI 的消息
#[derive(Debug)]
pub enum Msg {
    /// 容器列表刷新结果
    Containers(Result<Vec<ContainerInfo>, String>),
    /// 镜像列表刷新结果
    Images(Result<Vec<ImageInfo>, String>),
    /// 一次性资源采样结果（容器 ID, 采样结果）
    Sample(String, Result<ResourceSample, String>),
    /// 拉取镜像进度文本
    PullProgress(String),
    /// 拉取镜像完成
    PullDone(Result<String, String>),
    /// 启动/停止/重启/删除等操作完成（操作描述, 结果）
    ActionDone(String, Result<String, String>),
    /// 清理完成
    CleanupDone(Result<String, String>),
}

/// 单次资源采样结果（一次性 stats 请求的返回）
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceSample {
    /// CPU 使用率（%）
    pub cpu_percent: f64,
    /// 内存使用量（字节）
    pub mem_usage: u64,
    /// 内存上限（字节）
    pub mem_limit: u64,
    /// 磁盘读累计（字节）
    pub block_read: u64,
    /// 磁盘写累计（字节）
    pub block_write: u64,
    /// 网络接收累计（字节）
    pub net_rx: u64,
    /// 网络发送累计（字节）
    pub net_tx: u64,
}
