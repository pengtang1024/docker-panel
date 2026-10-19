//! 容器详情 + 日志流：inspect_container + follow_logs

use std::time::Duration;

use crate::models::ContainerDetail;
use crate::models::Msg;

use super::connect;

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

/// 日志流结束原因（决定是否退避重连）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogStreamEnd {
    /// 被取消（切容器 / Clear 重建流）
    Cancelled,
    /// 流自然结束（容器停止 / 日志 EOF）：无需重连
    Eof,
    /// Docker 侧错误（daemon 重启、连接抖动等）：可退避重连
    Error(String),
}

/// 重连退避序列：1s → 2s → 4s → 8s → 10s（上限）
pub fn retry_delay(attempt: u32) -> Duration {
    const MAX: Duration = Duration::from_secs(10);
    if attempt >= 4 {
        return MAX;
    }
    Duration::from_secs(1u64 << attempt).min(MAX)
}

/// follow_logs — 持续读容器日志，每行通过 tx.send(Msg::LogLine(epoch, line)) 推给 consumer
///
/// `cancel` 由 AppState 持有 sender；换容器/清空时 drop sender，
/// 本任务会在下一次 `select!` 立即退出，避免旧容器的日志流泄漏并污染新容器。
/// `epoch` 随每条日志回传；consumer 只接受与 AppState 当前 stream epoch 相同的行，
/// 用来丢弃切换/清空瞬间已经排进 mpsc 队列的旧日志。
pub async fn follow_logs(
    id: &str,
    tx: std::sync::mpsc::Sender<Msg>,
    tail_lines: i64,
    epoch: u64,
    mut cancel: tokio::sync::watch::Receiver<()>,
) -> LogStreamEnd {
    use bollard::container::LogOutput;
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;

    let docker = match connect() {
        Ok(d) => d,
        Err(e) => return LogStreamEnd::Error(e),
    };
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(&tail_lines.to_string())
        .build();

    let mut stream = docker.logs(id, Some(options));

    loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => return LogStreamEnd::Cancelled,
            item = stream.next() => {
                let Some(item) = item else { return LogStreamEnd::Eof };
                let msg = match item {
                    Ok(m) => m,
                    Err(e) => return LogStreamEnd::Error(e.to_string()),
                };
                let data = match msg {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => message,
                    _ => continue,
                };
                let line = String::from_utf8_lossy(&data).trim().to_string();
                if !line.is_empty() {
                    let _ = tx.send(Msg::LogLine(epoch, line));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_backs_off_and_caps() {
        assert_eq!(retry_delay(0), Duration::from_secs(1));
        assert_eq!(retry_delay(1), Duration::from_secs(2));
        assert_eq!(retry_delay(2), Duration::from_secs(4));
        assert_eq!(retry_delay(3), Duration::from_secs(8));
        assert_eq!(retry_delay(4), Duration::from_secs(10));
        assert_eq!(retry_delay(99), Duration::from_secs(10));
    }

    #[test]
    fn follow_logs_reports_error_for_unknown_container() {
        // 需要本机 Docker 守护进程；容器不存在时日志流返回 Error（可退避重连），
        // 而不是 Eof（容器停止，不重连）。
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let (tx, _rx) = std::sync::mpsc::channel();
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(());
        let end = rt.block_on(follow_logs(
            "zz-dp-nonexistent-container",
            tx,
            0,
            1,
            cancel_rx,
        ));
        assert!(matches!(end, LogStreamEnd::Error(_)), "应判定为可重连错误");
    }
}
