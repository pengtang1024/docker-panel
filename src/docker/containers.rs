//! 容器相关 API：列表、启停重启删除、inspect、日志流

use bollard::query_parameters::ListContainersOptionsBuilder;

use crate::models::{ContainerDetail, ContainerInfo};

use super::connect;

/// 端口映射列表 → "0.0.0.0:6399->6379/tcp, ..." 文本
fn format_ports(ports: Option<&Vec<bollard::models::PortSummary>>) -> String {
    let Some(ports) = ports else {
        return String::new();
    };
    let items: Vec<String> = ports
        .iter()
        .map(|p| {
            let proto = p
                .typ
                .as_ref()
                .map(std::string::ToString::to_string)
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

/// 拉取容器列表
///
/// - `all=true` 时包含已停止容器
/// - `size=true` 让 Docker 返回 SizeRw / SizeRootFs（略慢，遍历文件系统）
/// - compose label 提取：`com.docker.compose.project` / `com.docker.compose.service`
///   用于 v0.1 的 Compose 分组功能，v0.2 UI 不展示但数据仍保留（v0.3 计划加回分组）
/// - 资源字段（cpu_percent、mem_usage 等）这里填 0——资源实时采集（stats）
///   在 v0.1 有、v0.2 移除以简化功能，UI 直接展示 0
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
            let state = c.state.map_or_else(|| "unknown".into(), |s| s.to_string());
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
                ports: format_ports(c.ports.as_ref()),
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

/// inspect_container → 精简 ContainerDetail
pub async fn inspect_container(id: &str) -> Result<ContainerDetail, String> {
    let docker = connect()?;
    let raw = docker
        .inspect_container(id, None)
        .await
        .map_err(|e| format!("inspect 容器失败: {e}"))?;

    let name = raw.name.clone().unwrap_or_default();
    let name = name.trim_start_matches('/').to_string();
    let state = raw.state.clone().and_then(|s| s.status).unwrap_or_default();
    let (status, pid, exit_code, started_at, finished_at) = raw
        .state
        .map(|s| {
            (
                s.status.clone().unwrap_or_default(),
                s.pid.unwrap_or(0),
                s.exit_code.unwrap_or(0),
                s.started_at.unwrap_or_default(),
                s.finished_at.unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let cfg = raw.config.clone();
    let host_cfg = raw.host_config.clone();
    let net = raw.network_settings.clone();

    let ports: Vec<String> = net
        .ports
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| {
            let inner: String = v
                .map(|ps| {
                    ps.iter()
                        .map(|p| format!("{}:{}", p.host_ip.as_deref().unwrap_or("0.0.0.0"), p.host_port.as_deref().unwrap_or("?")))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            if inner.is_empty() { k } else { format!("{inner}->{k}") }
        })
        .collect();

    let mounts: Vec<String> = raw
        .mounts
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            format!(
                "{}:{}{}",
                m.source.unwrap_or_default(),
                m.destination.unwrap_or_default(),
                if m.rw.unwrap_or(false) { "" } else { ":ro" }
            )
        })
        .collect();

    let env: Vec<String> = cfg.as_ref().and_then(|c| c.env.clone()).unwrap_or_default();
    let labels: Vec<(String, String)> = cfg
        .as_ref()
        .and_then(|c| c.labels.clone())
        .map(|h| h.into_iter().map(|(k, v)| (k, v.unwrap_or_default())).collect())
        .unwrap_or_default();

    let command = cfg
        .as_ref()
        .and_then(|c| c.cmd.clone())
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let working_dir = cfg.as_ref().and_then(|c| c.working_dir.clone()).unwrap_or_default();
    let entrypoint = cfg
        .as_ref()
        .and_then(|c| c.entrypoint.clone())
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let restart_policy = host_cfg
        .as_ref()
        .and_then(|h| h.restart_policy.clone())
        .and_then(|r| r.name)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let network_mode = host_cfg
        .as_ref()
        .and_then(|h| h.network_mode.clone())
        .unwrap_or_default();
    let log_path = raw.log_path.clone().unwrap_or_default();

    let networks: Vec<String> = net
        .networks
        .unwrap_or_default()
        .into_keys()
        .collect();

    Ok(ContainerDetail {
        id: raw.id.clone().unwrap_or_default(),
        name,
        image: cfg.as_ref().and_then(|c| c.image.clone()).unwrap_or_default(),
        image_id: raw.image.clone().unwrap_or_default(),
        created: raw.created.clone().unwrap_or_default(),
        state,
        status,
        pid,
        exit_code,
        started_at,
        finished_at,
        ports,
        mounts,
        env,
        labels,
        command,
        working_dir,
        entrypoint,
        restart_policy,
        log_path,
        network_mode,
        networks,
    })
}

/// follow_logs — 持续读容器日志，每行通过 tx.send(Msg::LogLine) 推给 consumer
///
/// - `tail_lines`: 启动时先拉的历史行（-1 = 全部）
/// - 返回值是一个 stream，但在函数内部就循环消费并 send，
///   调用方只需要 await 它在后台运行，不需要再自己 poll
pub async fn follow_logs(
    id: &str,
    tx: std::sync::mpsc::Sender<crate::models::Msg>,
    tail_lines: i64,
) -> Result<(), String> {
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;

    let docker = connect()?;
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(tail_lines.to_string())
        .build();

    let mut stream = docker.logs(id, Some(options));

    while let Some(Ok(msg)) = stream.next().await {
        // 丢弃 header 字节（1 字节 stream type + 3 字节 padding + 4 字节 length）
        let data = &msg[..];
        let mut pos = 0;
        while pos + 8 <= data.len() {
            let length =
                u32::from_be_bytes(data[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            pos += 8;
            let end = (pos + length).min(data.len());
            let line = String::from_utf8_lossy(&data[pos..end]).trim().to_string();
            if !line.is_empty() {
                let _ = tx.send(crate::models::Msg::LogLine(line));
            }
            pos = end;
        }
    }
    Ok(())
}
