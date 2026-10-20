//! 容器详情（inspect）+ 存活性探针（probe）
//!
//! 探针是日志流自愈的**权威判定**：容器是否还在跑、Docker 是否可达，
//! 全部由这里回答，取代跨流共享的 running 标志（旧实现会被旧流污染）。
//! 状态文本 → 结论的映射是纯函数，单测无需真实 Docker。

use crate::models::ContainerDetail;

use super::connect;
use super::log_types::LogError;
use super::CALL_TIMEOUT;

/// 容器可达性探针结论
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerReach {
    /// 容器在运行（running / restarting / paused）
    Running,
    /// 容器已停止（created / exited / dead ...）
    Stopped,
    /// 容器已不存在（HTTP 404）
    Gone,
    /// Docker 不可达（连接失败 / 超时）
    Unreachable,
}

/// 容器状态文本 → 探针结论（纯函数）
///
/// running / restarting / paused 都视为「还在跑」：paused 容器日志流仍可读，
/// restarting 容器只是处于重启间隙，两者都应继续跟随。
pub fn reach_from_state(state: &str) -> ContainerReach {
    match state {
        "running" | "restarting" | "paused" => ContainerReach::Running,
        _ => ContainerReach::Stopped,
    }
}

/// 探针：容器当前是否还活着
///
/// - 连接失败 / 超时 → `Unreachable`（Docker 整体不可用）
/// - HTTP 404 → `Gone`（容器已被删除）
/// - 其它状态 → `Running` / `Stopped`
pub async fn probe_container(id: &str) -> ContainerReach {
    let docker = match connect() {
        Ok(d) => d,
        Err(_) => return ContainerReach::Unreachable,
    };
    match tokio::time::timeout(CALL_TIMEOUT, docker.inspect_container(id, None)).await {
        Ok(Ok(raw)) => {
            let state = raw
                .state
                .as_ref()
                .and_then(|s| s.status.as_ref())
                .map(|s| s.to_string())
                .unwrap_or_default();
            reach_from_state(&state)
        }
        Ok(Err(e)) => {
            let err = LogError::from_bollard(&e);
            if err.is_fatal() {
                ContainerReach::Gone
            } else {
                ContainerReach::Unreachable
            }
        }
        Err(_) => ContainerReach::Unreachable,
    }
}

/// inspect_container → 精简 ContainerDetail
pub async fn inspect_container(id: &str) -> Result<ContainerDetail, String> {
    let docker = connect()?;
    let raw = docker
        .inspect_container(id, None)
        .await
        .map_err(|e| crate::i18n::trf("inspect container failed: {0}", &[&e.to_string()]))?;

    let name = raw.name.clone().unwrap_or_default();
    let name = name.trim_start_matches('/').to_string();

    let (state, status, pid, exit_code, started_at, finished_at) = raw
        .state
        .map(|s| {
            let st = s.status.as_ref().map(|e| e.to_string()).unwrap_or_default();
            (
                st.clone(),
                st,
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
        .as_ref()
        .and_then(|n| n.ports.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| {
            let inner: String = v
                .map(|ps| {
                    ps.iter()
                        .map(|p| {
                            let ip = p.host_ip.as_deref().unwrap_or("0.0.0.0");
                            let port = p.host_port.as_deref().unwrap_or("?");
                            format!("{ip}:{port}")
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            if inner.is_empty() {
                k
            } else {
                format!("{inner}->{k}")
            }
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
        .map(|h| h.into_iter().collect())
        .unwrap_or_default();

    let command = cfg
        .as_ref()
        .and_then(|c| c.cmd.clone())
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let working_dir = cfg
        .as_ref()
        .and_then(|c| c.working_dir.clone())
        .unwrap_or_default();
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
        .and_then(|n| n.networks)
        .unwrap_or_default()
        .into_keys()
        .collect();

    Ok(ContainerDetail {
        id: raw.id.clone().unwrap_or_default(),
        name,
        image: cfg
            .as_ref()
            .and_then(|c| c.image.clone())
            .unwrap_or_default(),
        image_id: raw.image.clone().unwrap_or_default(),
        created: raw.created.clone().unwrap_or_default(),
        state,
        status,
        pid,
        exit_code,
        restarts: raw.restart_count.unwrap_or(0),
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
