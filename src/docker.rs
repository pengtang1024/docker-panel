//! bollard 封装：所有 Docker API 调用都通过本模块发起
//!
//! 约定：
//! - 连接地址不硬编码，统一使用 `Docker::connect_with_local_defaults()`
//!   （自动读取 DOCKER_HOST，默认本机 unix socket /var/run/docker.sock）
//! - 所有函数都是 async，由上层 tokio 运行时在后台线程执行，不阻塞 GUI

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use bollard::query_parameters::{
    ListContainersOptionsBuilder, ListImagesOptionsBuilder, LogsOptionsBuilder,
    PruneImagesOptionsBuilder, RemoveImageOptionsBuilder, StatsOptionsBuilder,
};
use bollard::Docker;
use futures::{StreamExt, TryStreamExt};

use crate::models::{ContainerInfo, ImageInfo, LogEvent, ResourceSample};

/// 建立本机 Docker 连接
pub fn connect() -> Result<Docker, String> {
    Docker::connect_with_local_defaults().map_err(|e| format!("连接 Docker 失败: {e}"))
}

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
    let options = ListContainersOptionsBuilder::new().all(all).build();
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
                net_rx: 0,
                net_tx: 0,
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

/// 拉取镜像列表，同时统计每个镜像关联的容器数量
pub async fn list_images() -> Result<Vec<ImageInfo>, String> {
    let docker = connect()?;
    let options = ListImagesOptionsBuilder::new().all(true).build();
    let images = docker
        .list_images(Some(options))
        .await
        .map_err(|e| format!("获取镜像列表失败: {e}"))?;
    let containers = list_containers(true).await.unwrap_or_default();

    // 镜像名:tag → 关联容器数（在 image_id 匹配不到时兜底）
    let mut count: HashMap<String, i64> = HashMap::new();
    for c in &containers {
        *count.entry(c.image.clone()).or_insert(0) += 1;
    }

    Ok(images
        .into_iter()
        .map(|img| {
            let id = img.id.clone();
            let short = id.trim_start_matches("sha256:").to_string();
            // 关联容器：优先按 image_id 精确匹配，其次按镜像名匹配
            let by_id = containers
                .iter()
                .filter(|c| {
                    let cid = c.image_id.trim_start_matches("sha256:");
                    !cid.is_empty() && (c.image_id == id || cid == short)
                })
                .count() as i64;
            let by_name = if by_id > 0 {
                0
            } else {
                img.repo_tags
                    .iter()
                    .map(|t| count.get(t.as_str()).copied().unwrap_or(0))
                    .sum()
            };
            let (name, tag) = match img.repo_tags.first() {
                Some(rt) => match rt.rsplit_once(':') {
                    Some((n, t)) => (n.to_string(), t.to_string()),
                    None => (rt.clone(), "latest".to_string()),
                },
                None => ("<none>".to_string(), "<none>".to_string()),
            };
            ImageInfo {
                id,
                name,
                tag,
                size: img.size,
                created: img.created,
                containers: by_id + by_name,
                dangling: img.repo_tags.is_empty()
                    || img.repo_tags.iter().any(|t| t == "<none>:<none>"),
            }
        })
        .collect())
}

/// 删除镜像
pub async fn remove_image(name: &str, tag: &str) -> Result<String, String> {
    let docker = connect()?;
    let reference = if tag.is_empty() || tag == "<none>" {
        name.to_string()
    } else {
        format!("{name}:{tag}")
    };
    let options = RemoveImageOptionsBuilder::new().force(true).build();
    docker
        .remove_image(&reference, Some(options), None)
        .await
        .map_err(|e| format!("删除镜像失败: {e}"))?;
    Ok(format!("镜像 {reference} 已删除"))
}

/// 拉取镜像（spec 形如 "nginx:latest"，进度通过 on_progress 回调上报）
pub async fn pull_image(
    spec: &str,
    on_progress: impl Fn(String),
) -> Result<String, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("请输入镜像名称，例如 nginx:latest".into());
    }
    let docker = connect()?;

    // 拆分镜像名与 tag（最后一个 ':' 需位于最后一段路径之后，兼容 registry:5000/img）
    let (name, tag) = match spec.rsplit_once(':') {
        Some((n, t)) if !n.is_empty() && !t.contains('/') => (n.to_string(), t.to_string()),
        _ => (spec.to_string(), "latest".to_string()),
    };

    let options = bollard::query_parameters::CreateImageOptionsBuilder::new()
        .from_image(&name)
        .tag(&tag)
        .build();

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(item) = stream.next().await {
        match item {
            Ok(info) => {
                if let Some(err) = info.error_detail.as_ref().and_then(|e| e.message.clone()) {
                    return Err(format!("拉取镜像失败: {err}"));
                }
                let mut text = info.status.unwrap_or_default();
                if let Some(detail) = info.progress_detail {
                    if let (Some(cur), Some(total)) = (detail.current, detail.total) {
                        if total > 0 {
                            text = format!("{text} ({:.1}%)", cur as f64 / total as f64 * 100.0);
                        }
                    }
                }
                if !text.trim().is_empty() {
                    on_progress(text);
                }
            }
            Err(e) => return Err(format!("拉取镜像失败: {e}")),
        }
    }
    Ok(format!("{name}:{tag} 拉取完成"))
}

/// 清理：停止的容器 / 悬空镜像 / 未使用镜像（可组合勾选）
pub async fn cleanup(stopped: bool, dangling: bool, unused: bool) -> Result<String, String> {
    let docker = connect()?;
    let mut lines: Vec<String> = Vec::new();

    if stopped {
        let resp = docker
            .prune_containers(None)
            .await
            .map_err(|e| format!("清理停止容器失败: {e}"))?;
        let n = resp.containers_deleted.as_ref().map(|v| v.len()).unwrap_or(0);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "停止的容器：删除 {n} 个，释放 {}",
            crate::utils::format_bytes(space as u64)
        ));
    }

    if dangling && !unused {
        let mut filters = HashMap::new();
        filters.insert("dangling".to_string(), vec!["true".to_string()]);
        let options = PruneImagesOptionsBuilder::new().filters(&filters).build();
        let resp = docker
            .prune_images(Some(options))
            .await
            .map_err(|e| format!("清理悬空镜像失败: {e}"))?;
        let n = resp
            .images_deleted
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "悬空镜像：删除 {n} 项，释放 {}",
            crate::utils::format_bytes(space as u64)
        ));
    }

    if unused {
        // dangling=false 等价 docker image prune -a：清理所有未被容器使用的镜像
        let mut filters = HashMap::new();
        filters.insert("dangling".to_string(), vec!["false".to_string()]);
        let options = PruneImagesOptionsBuilder::new().filters(&filters).build();
        let resp = docker
            .prune_images(Some(options))
            .await
            .map_err(|e| format!("清理未使用镜像失败: {e}"))?;
        let n = resp
            .images_deleted
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "未使用镜像：删除 {n} 项，释放 {}",
            crate::utils::format_bytes(space as u64)
        ));
    }

    if lines.is_empty() {
        return Err("请至少勾选一项清理内容".into());
    }
    Ok(lines.join("\n"))
}

/// 持续读取容器日志流，逐行回传；流结束时发送 LogEvent::Closed
pub async fn follow_logs(
    id: &str,
    tail: usize,
    tx: Sender<LogEvent>,
) -> Result<(), String> {
    let docker = connect()?;
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(&tail.to_string())
        .build();
    let mut stream = docker.logs(id, Some(options));

    while let Some(item) = stream.next().await {
        match item {
            Ok(out) => {
                let text = out.to_string();
                for line in text.split('\n') {
                    let line = line.trim_end_matches('\r');
                    if !line.is_empty() && tx.send(LogEvent::Line(line.to_string())).is_err() {
                        return Ok(()); // 界面已关闭
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(LogEvent::Line(format!("[日志读取错误] {e}")));
                break;
            }
        }
    }
    let _ = tx.send(LogEvent::Closed);
    Ok(())
}
