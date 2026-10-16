//! 容器详情 + 日志流：inspect_container + follow_logs

use crate::models::ContainerDetail;
use crate::models::Msg;

use super::connect;

/// inspect_container → 精简 ContainerDetail
pub async fn inspect_container(id: &str) -> Result<ContainerDetail, String> {
    let docker = connect()?;
    let raw = docker
        .inspect_container(id, None)
        .await
        .map_err(|e| format!("inspect 容器失败: {e}"))?;

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
        .map(|h| h.into_iter().map(|(k, v)| (k, v)).collect())
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
pub async fn follow_logs(
    id: &str,
    tx: std::sync::mpsc::Sender<Msg>,
    tail_lines: i64,
) -> Result<(), String> {
    use bollard::container::LogOutput;
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;

    let docker = connect()?;
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(&tail_lines.to_string())
        .build();

    let mut stream = docker.logs(id, Some(options));

    while let Some(Ok(msg)) = stream.next().await {
        let data = match msg {
            LogOutput::StdOut { message } | LogOutput::StdErr { message } => message,
            _ => continue,
        };
        let line = String::from_utf8_lossy(&data).trim().to_string();
        if !line.is_empty() {
            let _ = tx.send(Msg::LogLine(line));
        }
    }
    Ok(())
}
