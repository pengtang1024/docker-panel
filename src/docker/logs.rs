//! 容器日志流：follow_logs（持续读取 + 行级推送 + 空闲超时）
//!
//! 结束原因 / 错误分类在 `log_types.rs`，容器详情与探针在 `container_detail.rs`，
//! 重连策略在 `follow_runtime.rs`。本模块只负责「把 Docker 的日志流搬成 Msg」。

use std::sync::mpsc::Sender;

use crate::log_status::NOTICE_RECOVERED;
use crate::models::Msg;

use super::connect;
use super::log_types::{LogError, LogStreamEnd, LogStreamOutcome, LOG_IDLE_TIMEOUT};

/// follow_logs — 持续读容器日志，每行通过 tx.send(Msg::LogLine(epoch, line)) 推给 consumer
///
/// 返回 `LogStreamOutcome`：
/// - `end`：`Cancelled`（被取消）/ `Eof`（容器停止或 Docker 侧干净关闭）/
///   `Idle`（超过 `LOG_IDLE_TIMEOUT` 无新行）/ `Error`（Docker 侧错误）
/// - `lines`：本次连接推送出的日志行数，调用方据此决定重连时是否重放历史
/// - `notified_recovered`：本次连接是否已提示「已恢复连接」
///
/// `cancel` 由 AppState 持有 sender；换容器/清空时 drop sender，
/// 本任务会在下一次 `select!` 立即退出，避免旧容器的日志流泄漏并污染新容器。
/// `epoch` 随每条日志回传；consumer 只接受与 AppState 当前 stream epoch 相同的行，
/// 用来丢弃切换/清空瞬间已经排进 mpsc 队列的旧日志。
/// `notify_recovered` 为真时，本次连接收到首行日志即提示「已恢复连接」
/// （只有此前提示过「正在重连」时调用方才会置真）。
/// 重连与退避策略由调用方（`follow_runtime`）负责。
pub async fn follow_logs(
    id: &str,
    tx: Sender<Msg>,
    tail_lines: i64,
    epoch: u64,
    mut cancel: tokio::sync::watch::Receiver<()>,
    notify_recovered: bool,
) -> LogStreamOutcome {
    use bollard::container::LogOutput;
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;

    let docker = match connect() {
        Ok(d) => d,
        Err(e) => {
            return LogStreamOutcome {
                end: LogStreamEnd::Error(LogError {
                    message: e,
                    status_code: None,
                }),
                lines: 0,
                notified_recovered: false,
            }
        }
    };
    let options = LogsOptionsBuilder::new()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail(&tail_lines.to_string())
        .build();

    let mut stream = docker.logs(id, Some(options));
    let mut lines: u64 = 0;
    let mut notified_recovered = false;

    loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => {
                return LogStreamOutcome { end: LogStreamEnd::Cancelled, lines, notified_recovered };
            }
            item = tokio::time::timeout(LOG_IDLE_TIMEOUT, stream.next()) => {
                let item = match item {
                    // 长时间无任何日志：连接可能挂死，主动断流走重连路径
                    Err(_elapsed) => {
                        return LogStreamOutcome { end: LogStreamEnd::Idle, lines, notified_recovered };
                    }
                    Ok(item) => item,
                };
                let Some(item) = item else {
                    return LogStreamOutcome { end: LogStreamEnd::Eof, lines, notified_recovered };
                };
                let msg = match item {
                    Ok(m) => m,
                    Err(e) => {
                        return LogStreamOutcome {
                            end: LogStreamEnd::Error(LogError::from_bollard(&e)),
                            lines,
                            notified_recovered,
                        };
                    }
                };
                let data = match msg {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => message,
                    _ => continue,
                };
                let line = String::from_utf8_lossy(&data).trim().to_string();
                if !line.is_empty() {
                    let _ = tx.send(Msg::LogLine(epoch, line));
                    lines += 1;
                    // 重连后的首行日志：确认连接已恢复，提示用户
                    if notify_recovered && !notified_recovered {
                        notified_recovered = true;
                        let _ = tx.send(Msg::LogStreamStatus(epoch, crate::i18n::t(NOTICE_RECOVERED)));
                    }
                }
            }
        }
    }
}
