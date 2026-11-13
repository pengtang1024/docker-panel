//! 容器相关 API：列表、启停重启删除
//! 详情/日志 → logs.rs

use bollard::query_parameters::ListContainersOptionsBuilder;

use crate::models::ContainerInfo;

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
/// - **不带 `size`**：`size=1` 要为每个容器遍历文件系统（24 容器实测 70ms vs 32ms），
///   而且容器正在删除时会命中 dockerd `404 rw layer snapshot not found` 导致整批失败。
///   磁盘列改由 30s 的 `/system/df?type=container` 回填（`AppState::apply_container_sizes`）
pub async fn list_containers(all: bool) -> Result<Vec<ContainerInfo>, String> {
    let docker = connect()?;
    let options = ListContainersOptionsBuilder::new().all(all).build();
    let list = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| crate::i18n::trf("Failed to fetch container list: {0}", &[&e.to_string()]))?;

    Ok(list
        .into_iter()
        .map(|c| {
            let state = c.state.map_or_else(|| "unknown".into(), |s| s.to_string());
            let labels = c.labels.unwrap_or_default();
            let compose_project = labels.get("com.docker.compose.project").cloned();
            let compose_service = labels.get("com.docker.compose.service").cloned();
            let networks: Vec<String> = c
                .network_settings
                .and_then(|ns| ns.networks)
                .map(|m| m.into_keys().collect())
                .unwrap_or_default();
            // 具名卷（含匿名卷）：bind mount 没有 Name，不参与磁盘占用统计
            let volumes: Vec<String> = c
                .mounts
                .unwrap_or_default()
                .into_iter()
                .filter_map(|m| m.name)
                .collect();
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
                size_rw: 0,
                net_rx: 0,
                net_tx: 0,
                compose_project,
                compose_service,
                networks,
                volumes,
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
        .map_err(|e| crate::i18n::trf("Failed to start container: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Container started"))
}

/// 停止容器
pub async fn stop_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .stop_container(id, None)
        .await
        .map_err(|e| crate::i18n::trf("Failed to stop container: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Container stopped"))
}

/// 重启容器
pub async fn restart_container(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .restart_container(id, None)
        .await
        .map_err(|e| crate::i18n::trf("Failed to restart container: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Container restarted"))
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
        .map_err(|e| crate::i18n::trf("Failed to remove container: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Container removed"))
}
