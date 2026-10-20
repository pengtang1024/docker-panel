//! 日志流运行时纯函数单测
//!
//! 全部断言基于**纯函数**（结局分类 / 退避序列 / 错误致命性 / 探针映射），
//! 不依赖真实 Docker 守护进程，也不会挂起等待网络。

use std::time::Duration;

use super::*;
use crate::docker::container_detail::reach_from_state;
use crate::docker::log_types::LogError;

/// 构造一个连接类错误（无 HTTP 状态码）
fn conn_error(msg: &str) -> LogError {
    LogError {
        message: msg.to_string(),
        status_code: None,
    }
}

/// 构造一个容器已删除的错误（HTTP 404）
fn gone_error() -> LogError {
    LogError {
        message: "No such container: abc".to_string(),
        status_code: Some(404),
    }
}

#[test]
fn follow_tail_avoids_replaying_history() {
    assert_eq!(follow_tail(false), 200);
    assert_eq!(follow_tail(true), 0);
}

#[test]
fn cancelled_always_ends_without_prompting() {
    assert_eq!(
        classify_end(&LogStreamEnd::Cancelled, ContainerReach::Running),
        FollowAction::End
    );
}

#[test]
fn eof_ends_only_when_container_is_gone_or_stopped() {
    // 容器已停止 / 已删除 → 日志流本就该结束
    assert_eq!(
        classify_end(&LogStreamEnd::Eof, ContainerReach::Stopped),
        FollowAction::End
    );
    assert_eq!(
        classify_end(&LogStreamEnd::Eof, ContainerReach::Gone),
        FollowAction::End
    );
    // 仍在运行却 EOF → 可恢复（自愈）
    assert!(matches!(
        classify_end(&LogStreamEnd::Eof, ContainerReach::Running),
        FollowAction::Recover { .. }
    ));
    // Docker 不可达 → 等它回来
    assert!(matches!(
        classify_end(&LogStreamEnd::Eof, ContainerReach::Unreachable),
        FollowAction::Recover { .. }
    ));
}

#[test]
fn idle_is_keepalive_not_failure() {
    // 空闲超时走保活：不打扰用户、不计失败预算
    assert!(matches!(
        classify_end(&LogStreamEnd::Idle, ContainerReach::Running),
        FollowAction::KeepAlive { .. }
    ));
    assert!(matches!(
        classify_end(&LogStreamEnd::Idle, ContainerReach::Unreachable),
        FollowAction::KeepAlive { .. }
    ));
}

#[test]
fn fatal_error_ends_but_recoverable_error_reconnects() {
    // 容器已删除（404）→ 结束
    assert_eq!(
        classify_end(&LogStreamEnd::Error(gone_error()), ContainerReach::Running),
        FollowAction::End
    );
    // 连接被拒 → 重连
    assert!(matches!(
        classify_end(
            &LogStreamEnd::Error(conn_error("Connection refused")),
            ContainerReach::Running
        ),
        FollowAction::Recover { .. }
    ));
    // 连接被拒但容器已停止 → 没有可跟随的流 → 结束
    assert_eq!(
        classify_end(
            &LogStreamEnd::Error(conn_error("Connection refused")),
            ContainerReach::Stopped
        ),
        FollowAction::End
    );
}

#[test]
fn error_without_fatal_code_is_recoverable() {
    // 500 之类的服务端错误不是致命错误
    let e = LogError {
        message: "server error".into(),
        status_code: Some(500),
    };
    assert!(!e.is_fatal());
    assert!(matches!(
        classify_end(&LogStreamEnd::Error(e), ContainerReach::Running),
        FollowAction::Recover { .. }
    ));
}

#[test]
fn fatal_detection_covers_404_and_text() {
    assert!(gone_error().is_fatal());
    // 无状态码但文本命中
    let by_text = LogError {
        message: "No such container: xyz".into(),
        status_code: None,
    };
    assert!(by_text.is_fatal());
    assert!(!conn_error("Connection refused").is_fatal());
}

#[test]
fn retry_delay_follows_capped_backoff() {
    assert_eq!(retry_delay(0), Duration::from_secs(1));
    assert_eq!(retry_delay(1), Duration::from_secs(2));
    assert_eq!(retry_delay(2), Duration::from_secs(4));
    assert_eq!(retry_delay(3), Duration::from_secs(8));
    assert_eq!(retry_delay(4), Duration::from_secs(10));
    assert_eq!(retry_delay(99), Duration::from_secs(10));
}

#[test]
fn reach_from_state_maps_running_family_to_running() {
    assert_eq!(reach_from_state("running"), ContainerReach::Running);
    assert_eq!(reach_from_state("restarting"), ContainerReach::Running);
    assert_eq!(reach_from_state("paused"), ContainerReach::Running);
    assert_eq!(reach_from_state("exited"), ContainerReach::Stopped);
    assert_eq!(reach_from_state("created"), ContainerReach::Stopped);
    assert_eq!(reach_from_state("dead"), ContainerReach::Stopped);
    assert_eq!(reach_from_state(""), ContainerReach::Stopped);
}
