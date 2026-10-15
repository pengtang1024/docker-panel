//! 容器相关 API：列表、资源采样、启停重启删除

use bollard::query_parameters::{ListContainersOptionsBuilder, StatsOptionsBuilder};
use futures::TryStreamExt;

use crate::models::{ContainerInfo, ResourceSample};

use super::connect;

/// 端口映射列表 → "0.0.0.0:6399->6379/tcp, ..." 文本
fn format_ports(ports: &Option<Vec<bollard::models::PortSummary>>) -> String {
    let Some(ports) = ports else {
        return String::new();
    };
    let items: Vec<String> = ports
        .iter()
        .map(|p| {
            let proto = p
                .typ
                .as_ref()
                .map(|t| t.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "tcp".into());
            match (p.ip.as_deref(), p.public_port) {
                (Some(ip), Some(pub_port)) if !ip.is_empty() => {
                    format!("{ip}:{pub_port}->{}({})", p.private_port, proto)
                }
                (_, Some(pub_port)) => format!("{pub_port}->{}({})", p.private_port, proto),
                _ => format!("{}({})", p.private_port, proto),
            }
        })
        .collect();
    items.join(", ")
}

/// 拉取容器列表（all=true 时包含已停止容器）
pub async fn list_containers(all: bool) -> Result<Vec<ContainerInfo>, String> {
    let docker = connect()?;
    // size=true 让 Docker 返回 SizeRw/SizeRootFs（遍历文件系统，容器多时略慢）
    let options = ListContainersOptionsBuilder::new()
        .all(all)
        .size(true)
        .build();
    let list = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| format!("获取容器列表失败: {e}"))?;

    Ok(list
        .into_iter()
        .map(|c| {
            let state = c
                .state
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".into());
            // compose 通过 label 标记归属项目与服务
            let labels = c.labels.unwrap_or_default();
            let compose_project = labels.get("com.docker.compose.project").cloned();
            let compose_service = labels.get("com.docker.compose.service").cloned();
            let size_rw = c.size_rw.and_then(|v| u64::try_from(v).ok()).unwrap_or(0);
            let size_root_fs = c
                .size_root_fs
                .and_then(|v| u64::try_from(v).ok())
                .unwrap_or(0);
            ContainerInfo {
                id: c.id.unwrap_or_default(),
                name: c
                    .names
                    .unwrap_or_default()
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default(),
                image: c.image.unwrap_or_default(),
                image_id: c.image_id.unwrap_or_default(),
                state,
                status: c.status.unwrap_or_default(),
                created: c.created.unwrap_or(0),
                ports: format_ports(&c.ports),
                cpu_percent: 0.0,
                mem_usage: 0,
                mem_limit: 0,
                block_read: 0,
                block_write: 0,
                size_rw,
                size_root_fs,
                net_rx: 0,
                net_tx: 0,
                compose_project,
                compose_service,
            }
        })
        .collect())
}

/// 一次性采样容器资源（CPU / 内存 / 磁盘 IO / 网络 IO）
pub async fn sample_container(id: &str) -> Result<ResourceSample, String> {
    let docker = connect()?;
    // stream=false：只取一次采样后断开
    let options = StatsOptionsBuilder::new().stream(false).build();
    let stats = docker
        .stats(id, Some(options))
        .try_next()
        .await
        .map_err(|e| format!("获取资源统计失败: {e}"))?
        .ok_or_else(|| "资源统计返回为空".to_string())?;

    let cpu = stats.cpu_stats.as_ref();
    let precpu = stats.precpu_stats.as_ref();
    let mut cpu_percent = 0.0;
    if let (Some(cpu), Some(precpu)) = (cpu, precpu) {
        let cpu_total = cpu
            .cpu_usage
            .as_ref()
            .and_then(|u| u.total_usage)
            .unwrap_or(0);
        let precpu_total = precpu
            .cpu_usage
            .as_ref()
            .and_then(|u| u.total_usage)
            .unwrap_or(0);
        let system = cpu.system_cpu_usage.unwrap_or(0);
        let presystem = precpu.system_cpu_usage.unwrap_or(0);
        let online = cpu.online_cpus.unwrap_or(0) as f64;
        let online = if online > 0.0 { online } else { 1.0 };
        if system > presystem && cpu_total >= precpu_total {
            cpu_percent =
                (cpu_total - precpu_total) as f64 / (system - presystem) as f64 * online * 100.0;
        }
    }

    // 内存：扣除缓存文件（兼容 cgroup v1 / v2 的不同键名）
    let mut mem_usage = 0u64;
    let mut mem_limit = 0u64;
    if let Some(mem) = stats.memory_stats.as_ref() {
        mem_usage = mem.usage.unwrap_or(0);
        mem_limit = mem.limit.unwrap_or(0);
        if let Some(map) = mem.stats.as_ref() {
            let cache = map
                .get("inactive_file")
                .or_else(|| map.get("total_inactive_file"))
                .copied()
                .unwrap_or(0);
            mem_usage = mem_usage.saturating_sub(cache);
        }
    }

    // 磁盘读写（cgroup v1 才有明细，cgroup v2 下为 None，显示 0 与 docker stats 一致）
    let (mut block_read, mut block_write) = (0u64, 0u64);
    if let Some(entries) = stats
        .blkio_stats
        .as_ref()
        .and_then(|b| b.io_service_bytes_recursive.as_ref())
    {
        for e in entries {
            let value = e.value.unwrap_or(0);
            match e.op.as_deref() {
                Some("Read") => block_read += value,
                Some("Write") => block_write += value,
                _ => {}
            }
        }
    }

    // 网络 IO：汇总所有网卡
    let (mut net_rx, mut net_tx) = (0u64, 0u64);
    if let Some(networks) = stats.networks.as_ref() {
        for n in networks.values() {
            net_rx += n.rx_bytes.unwrap_or(0);
            net_tx += n.tx_bytes.unwrap_or(0);
        }
    }

    Ok(ResourceSample {
        cpu_percent,
        mem_usage,
        mem_limit,
        block_read,
        block_write,
        net_rx,
        net_tx,
    })
}

/// 启动容器
pub async fn start_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .start_container(id, None)
        .await
        .map_err(|e| format!("启动容器失败: {e}"))?;
    Ok("容器已启动".into())
}

/// 停止容器
pub async fn stop_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .stop_container(id, None)
        .await
        .map_err(|e| format!("停止容器失败: {e}"))?;
    Ok("容器已停止".into())
}

/// 重启容器
pub async fn restart_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .restart_container(id, None)
        .await
        .map_err(|e| format!("重启容器失败: {e}"))?;
    Ok("容器已重启".into())
}

/// 删除容器（force=true 保证运行中的容器也能删除）
pub async fn remove_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    let options = bollard::query_parameters::RemoveContainerOptionsBuilder::new()
        .force(true)
        .build();
    docker
        .remove_container(id, Some(options))
        .await
        .map_err(|e| format!("删除容器失败: {e}"))?;
    Ok("容器已删除".into())
}
