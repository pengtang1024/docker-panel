//! 系统级 API：清理（prune）与日志流

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use bollard::query_parameters::{LogsOptionsBuilder, PruneImagesOptionsBuilder};
use futures::StreamExt;

use crate::models::LogEvent;

use super::connect;

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
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
        let n = resp
            .containers_deleted
            .as_ref()
            .map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "停止的容器：删除 {n} 个，释放 {}",
            format_bytes(space as u64)
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
        let n = resp.images_deleted.as_ref().map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "悬空镜像：删除 {n} 项，释放 {}",
            format_bytes(space as u64)
        ));
    }

    if unused {
        let mut filters = HashMap::new();
        filters.insert("dangling".to_string(), vec!["false".to_string()]);
        let options = PruneImagesOptionsBuilder::new().filters(&filters).build();
        let resp = docker
            .prune_images(Some(options))
            .await
            .map_err(|e| format!("清理未使用镜像失败: {e}"))?;
        let n = resp.images_deleted.as_ref().map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(format!(
            "未使用镜像：删除 {n} 项，释放 {}",
            format_bytes(space as u64)
        ));
    }

    if lines.is_empty() {
        return Err("请至少勾选一项清理内容".into());
    }
    Ok(lines.join("\n"))
}

/// 持续读取容器日志流，逐行回传；流结束时发送 `LogEvent::Closed`
pub async fn follow_logs(id: &str, tail: usize, tx: Sender<LogEvent>) -> Result<(), String> {
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
                        return Ok(());
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
