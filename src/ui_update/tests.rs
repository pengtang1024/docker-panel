use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use super::*;
use crate::models::ContainerInfo;

#[test]
fn log_stream_status_sets_status_text() {
    let (state, rx) = AppState::new();
    let mut s = state.lock().unwrap();
    let tx = s.tx.clone();
    tx.send(Msg::LogStreamStatus("日志流中断".into())).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(res.status_text.as_deref(), Some("日志流中断"));
}

#[test]
fn follow_running_flag_tracks_container_state() {
    let (state, rx) = AppState::new();
    let mut s = state.lock().unwrap();
    s.follow_id = "c1".into();
    let tx = s.tx.clone();

    let running = ContainerInfo {
        id: "c1".into(),
        state: "running".into(),
        ..Default::default()
    };
    tx.send(Msg::Containers(Ok(vec![running]))).unwrap();
    s.drain_messages(&rx, None, 10);
    assert!(
        s.follow_running.load(Ordering::Acquire),
        "运行中应标记为可恢复"
    );

    let stopped = ContainerInfo {
        id: "c1".into(),
        state: "exited".into(),
        ..Default::default()
    };
    tx.send(Msg::Containers(Ok(vec![stopped]))).unwrap();
    s.drain_messages(&rx, None, 10);
    assert!(
        !s.follow_running.load(Ordering::Acquire),
        "已停止不应触发 EOF 自愈"
    );
}

#[test]
fn stale_processing_is_not_a_ui_change() {
    let mut res = DrainResult {
        processed: 42,
        ..Default::default()
    };
    assert!(!res.has_ui_changes());
    res.logs_changed = true;
    assert!(res.has_ui_changes());
    res.logs_changed = false;
    res.status_text = Some("x".into());
    assert!(res.has_ui_changes());
}

#[test]
fn stale_stream_epoch_is_dropped_after_clear() {
    let (state, rx) = AppState::new();
    let mut s = state.lock().unwrap();
    s.logs.push("old".into());
    s.stream_epoch = 5;
    s.clear_logs(); // stream epoch → 6，缓冲清空
    assert!(s.logs.is_empty());
    assert_eq!(s.stream_epoch, 6);
    assert!(!s.history_done);

    let tx = s.tx.clone();
    tx.send(Msg::LogLine(5, "stale".into())).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert!(s.logs.is_empty(), "旧 epoch 日志必须被丢弃");
    assert!(!res.logs_changed);

    tx.send(Msg::LogLine(6, "fresh".into())).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(s.logs, vec!["fresh".to_string()]);
    assert!(res.logs_changed);
    assert!(s.history_done, "收到首条日志后才算历史完成");

    // #4：当前流的结束消息清空 follow_id；旧 epoch 的结束消息不影响新流
    s.follow_id = "c1".into();
    tx.send(Msg::LogStreamEnded(6)).unwrap();
    s.drain_messages(&rx, None, 10);
    assert!(s.follow_id.is_empty());
    s.follow_id = "c2".into();
    tx.send(Msg::LogStreamEnded(5)).unwrap();
    s.drain_messages(&rx, None, 10);
    assert_eq!(s.follow_id, "c2");

    // #12：当前流结束同时释放 cancel sender
    s.follow_id = "c3".into();
    let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(());
    s.follow_cancel = Some(cancel_tx);
    s.follow_active = Some(std::sync::Arc::new(AtomicBool::new(true)));
    tx.send(Msg::LogStreamEnded(6)).unwrap();
    s.drain_messages(&rx, None, 10);
    assert!(s.follow_id.is_empty());
    assert!(s.follow_cancel.is_none());
    assert!(s.follow_active.is_none());

    // #6：无活动流时 Clear 也必须推进 buffer epoch，失效旧的 log_text
    s.follow_id.clear();
    s.logs.push("old".into());
    s.history_done = false;
    let before = s.log_epoch.load(Ordering::Acquire);
    s.clear_log_buffer();
    assert!(s.logs.is_empty());
    assert_eq!(s.log_epoch.load(Ordering::Acquire), before + 1);
    // #9：Clear 强制标记历史完成，切回日志 Tab 不会再拉 200 行
    assert!(s.history_done);

    // #15：有活动流时 Clear：stream_epoch +1、log_epoch +1、follow_id 保持
    s.follow_id = "c4".into();
    s.logs.push("x".into());
    let se = s.stream_epoch;
    let be = s.log_epoch.load(Ordering::Acquire);
    s.clear_log_buffer();
    assert_eq!(s.stream_epoch, se + 1);
    assert_eq!(s.log_epoch.load(Ordering::Acquire), be + 1);
    assert_eq!(s.follow_id, "c4");
    assert!(s.logs.is_empty());
    assert!(s.history_done);

    // #18：Clear 前的旧 epoch 行必须被丢弃，且不能改变 history_done
    tx.send(Msg::LogLine(se, "stale-after-clear".into()))
        .unwrap();
    s.drain_messages(&rx, None, 10);
    assert!(s.logs.is_empty());
    assert!(s.history_done);

    // #16：空缓冲 + 无活动流时 Clear 不置 history_done，保留后续 tail=200 重试
    s.follow_id.clear();
    s.logs.clear();
    s.history_done = false;
    s.clear_log_buffer();
    assert!(!s.history_done);

    // 切容器会重置历史标志
    s.clear_logs();
    assert!(!s.history_done);
}
