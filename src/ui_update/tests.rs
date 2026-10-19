use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use super::*;

#[test]
fn log_stream_status_sets_status_text() {
    let (state, rx) = AppState::new();
    let mut s = state.lock().unwrap();
    let tx = s.tx.clone();
    tx.send(Msg::LogStreamStatus(0, "日志流中断".into()))
        .unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(res.log_status.as_deref(), Some("日志流中断"));
    assert!(res.status_text.is_none(), "日志流提示不应占用状态栏");

    // 旧 epoch 的提示必须被丢弃，避免落到新容器
    s.stream_epoch = 5;
    tx.send(Msg::LogStreamStatus(4, "旧提示".into())).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert!(res.log_status.is_none());
}

#[test]
fn log_status_is_passed_through_without_dedupe() {
    let (state, rx) = AppState::new();
    let mut s = state.lock().unwrap();
    let tx = s.tx.clone();

    tx.send(Msg::LogStreamStatus(0, "正在重连".into())).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(res.log_status.as_deref(), Some("正在重连"));

    // 提示变化：推送
    tx.send(Msg::LogStreamStatus(0, "已恢复连接".into()))
        .unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(res.log_status.as_deref(), Some("已恢复连接"));

    // 相同文案再次到达也照常透传：清空旧提示的责任在 UI 侧
    // （ui_apply::clear_log_panel / clear_log_status），state 不再镜像 UI 文案。
    tx.send(Msg::LogStreamStatus(0, "已恢复连接".into()))
        .unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert_eq!(res.log_status.as_deref(), Some("已恢复连接"));
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
    tx.send(Msg::LogBatch(5, vec!["stale".into()])).unwrap();
    let res = s.drain_messages(&rx, None, 10);
    assert!(s.logs.is_empty(), "旧 epoch 日志必须被丢弃");
    assert!(!res.logs_changed);

    tx.send(Msg::LogBatch(6, vec!["fresh".into()])).unwrap();
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

    // #6：无活动流时 Clear 也必须推进 buffer epoch，失效旧的日志行增量
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
    tx.send(Msg::LogBatch(se, vec!["stale-after-clear".into()]))
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
