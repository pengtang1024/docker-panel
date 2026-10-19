# 运行时架构：线程模型 / 消息流 / 回调绑定

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是线程模型、消息流与回调纪律的权威深度版。

## 1. 线程模型（三线程架构）

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ UI 回调（搜索/排序/    │       │ recv_timeout(≤100ms) 循环 │       │ 3s 刷新 interval    │
│  操作/快捷键）──lock─▶│ 分批 drain（≤2048 条/批） │       │ 6s stats interval   │
│ 窗口关闭 → shutdown   │ pending.merge 聚合        │       │ 30s 卷占用 interval │
│   flag                │ 120ms 节流 apply          │◀──────│ bollard API 调用    │
│                        │ 只重建变化区域 → drop lock│       │ tx.send(Msg)        │
│                        │ invoke_from_event_loop     │       │ follow_logs 流      │
│                        │   render 缓存 + apply_drawer│      │ inspect_container   │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
   + shutdown: AtomicBool            （consumer 独占，不进 Mutex）
```

### Receiver 不进 Mutex — 死锁唯一保证

`mpsc::Receiver<Msg>` 由 consumer 线程独占所有权，**不**放在 `AppState` 里。
这样 `rx.recv_timeout(≤100ms)` 永远不会阻塞主线程拿 state 锁。
如果 Receiver 放进 Mutex，consumer 阻塞 `recv()` 时持有锁 → 主线程任何 `state.lock()`（UI 回调、interval tick）全卡死。

### 分批 drain + 120ms 节流（consumer.rs）

日志洪峰下，旧实现"每条 Msg 一次 invoke + 重建全部列表"会把 UI 线程压死，也会因长时间持锁饿死点击回调。当前实现分三层防护：

```rust
// Phase 1 — 锁外等待，锁内分批消费
// 有 pending 时只睡到 120ms 节流窗口结束（最多 100ms，保证 shutdown 可观测）
let wait = poll_timeout(pending.has_ui_changes(), last_apply.elapsed());
let first = rx.recv_timeout(wait)?;
loop {
    let res = {
        let Ok(mut s) = state.lock() else { return };
        s.drain_messages(&rx, first.take(), MAX_DRAIN_BATCH) // 单批 ≤2048
    };
    pending.merge(res);                            // 多批合并成一个 DrainResult
    if pending.shutdown { return; }
    if !pending.has_more || last_apply.elapsed() >= UI_MIN_INTERVAL { break; }
}

// Phase 2 — 每 120ms 最多应用一次 UI
if pending.has_ui_changes() && last_apply.elapsed() >= UI_MIN_INTERVAL {
    apply_ui(&state, &ui, &mut pending);
}
```

```rust
// apply_ui：锁内只 build 真正变化的区域 → drop lock → 锁外 invoke
let res = std::mem::take(pending);
let Ok(mut s) = state.lock() else { return };
if res.refresh_now { s.refresh(); }
let container_rows = res.lists_changed.then(|| ui_rows::build_container_rows(&s));
// ... image / network / volume / compose 同理
let drawer = (res.drawer_changed || res.lists_changed)
    .then(|| detail_fmt::build_drawer_data(&s));
let log_data = res.logs_changed.then(|| {
    let epoch = s.log_epoch.load(Ordering::Acquire);
    (SharedString::from(s.logs.join("\n")), s.log_epoch.clone(), epoch)
});
drop(s);  // ⚠️ 必须在 invoke_from_event_loop 之前释放

invoke_from_event_loop(move || {
    // render::containers(rows) 走 thread_local 缓存模型，只 set 变化的列表
    ui_apply::apply_drawer(&ui, &drawer);
    // 构建 log_text 后 UI 线程可能已 Clear/切容器：epoch 变了就丢弃旧文本
    if let Some((text, epoch_ref, epoch)) = log_data {
        if epoch_ref.load(Ordering::Acquire) == epoch {
            ui.set_log_text(text);
        }
    }
}).ok();
```

关键点：

- **单批上限**：`MAX_DRAIN_BATCH = 2048`，避免日志洪峰时无界 `try_recv` 长时间占锁。
- **UI 节流**：`UI_MIN_INTERVAL = 120ms`（约 8 次/秒）；有待更新时 `poll_timeout` 只睡到窗口结束（≤100ms，兼顾 shutdown），把最坏 UI 应用延迟从约 200ms 收敛到约 120ms。
- **变化区域**：`DrainResult` 只标记 `lists_changed / drawer_changed / logs_changed`；纯日志消息不会重算容器/镜像/网络/卷/抽屉。`has_ui_changes()` 为 false 时不 apply（被 epoch 丢弃的旧日志不会触发空 invoke）。
- **日志缓冲**：`LogLine(epoch, line)` 先推入 `AppState.logs`，整批 drain 后一次性裁剪到 500 行；UI 侧是一整个 `log_text` string，不再有逐行 Model/ListView。
- **日志流取消**：`AppState` 持有 `watch::Sender`；切换容器时 drop sender，旧 `follow_logs` 在 `select!` 中立即退出。日志行携带 stream epoch，consumer 只接受当前流；工具条 Clear 以 `tail=0` 重启当前流，清空前的排队日志按旧 epoch 丢弃。
- **日志续流**：日志缓冲代际与流代际分离。`log_epoch` 只在 clear/切容器时递增，用于 UI `log_text` 过期判断；`stream_epoch` 每次建流递增，用于日志行/结束消息。`history_done` 记录当前选中会话是否已加载历史：收到首条日志，或在有缓冲/活动流时 Clear 后置 true；空缓冲且无活动流时保持 false，让下次 `ensure_follow` 用 `tail=200` 重试。首次/空历史/首流失败用 `tail=200`，否则用 `tail=0`。重新选中容器（含同一容器）会重置，Clear 的持久性仅到下一次选中。
- **两阶段铁律**：锁内只 build，`invoke_from_event_loop` 闭包绝不碰 `Mutex<AppState>`；`log_epoch` 用共享 `Arc<AtomicU64>`，闭包只读原子判断文本是否已过期。

### 定时刷新走 tokio interval（不是 Slint Timer）

软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），定时任务全部改为 tokio interval：

- **3s 列表刷新**（`consumer::spawn_refresh_interval`）：锁内 clone handle → drop → spawn `interval(REFRESH_INTERVAL)` 循环，每 tick `s.refresh()` 并发刷新容器/镜像/网络/卷，结果走 `tx.send(Msg::*)`。
  - `refresh()` 带 in-flight 去重：上一轮未完成时本轮直接跳过，避免慢 Docker 下每 3s 再叠 4 个调用。
  - 所有列表/统计/inspect 调用都经 `docker::with_timeout`（10s），防止守护进程无响应时后台任务无限挂起、耗尽 tokio 工作线程。
- **6s stats 采样**（`bootstrap::spawn_stats_interval`）：join_all 拉所有 running 容器 stats，首帧跳过——双采样 CPU% 算法需要前一帧。
- **30s 卷占用**（`bootstrap::spawn_volume_size_interval`）：`/system/df` 遍历卷目录，低频单独跑。
- UI 回调与 interval tick 本身**不**碰 Slint Model——只发任务/改 state，consumer 统一调用 `invoke_from_event_loop` 更新 UI。
- 这符合 Slint 线程安全模型：只有主线程可以修改 Model。
- **主题监听可退出**：`dconf watch` 的阻塞读放在独立 reader 线程，监控线程用 `recv_timeout(200ms)` 轮询 shutdown；轮询回退路径也按 200ms 醒来检查。窗口关闭时 `theme-monitor` 能在 200ms 内 join，不会因 `reader.next()` 永久阻塞而让进程挂住。

## 2. 消息流（Msg 变体一览）

| 变体 | 触发源 | UI 行为 |
|------|--------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | 3s interval / 手动刷新 | 替换容器列表（保留旧 stats） |
| `Images(Result<Vec<ImageInfo>>)` | 3s interval / 手动刷新 | 替换镜像列表 + 后台补查层数 |
| `Networks(Result<Vec<NetworkInfo>>)` | 3s interval / 手动刷新 | 替换网络列表（网络视图） |
| `Volumes(Result<Vec<VolumeInfo>>)` | 3s interval / 手动刷新 | 替换卷列表（卷视图） |
| **`Stats(String id, ContainerStatsSnapshot)`** | 6s stats interval | 写入 `stats_prev` 双采样缓冲 → CPU%/内存/网络列 |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 关弹窗 + 标记立即刷新 |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本 + 标记立即刷新 |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 标记立即刷新 |
| **`ContainerDetail(Result<ContainerDetail>)`** | `inspect_container` → 详情面板 | 替换 `AppState.detail` → Drawer 详情 Tab |
| **`LogLine(u64, String)`** | `follow_logs` Stream | `(stream_epoch, line)`；不匹配当前流则丢弃 → 推入 `AppState.logs`（cap=500）→ `log_text` string |
| `LogStreamEnded(u64)` | follow_logs 任务结束 | 仅当前 stream_epoch 时清空 `follow_id` 并释放 cancel sender，允许下次重新建流 |
| `ImageDetail(Result<ImageDetail>)` | `inspect_image` → 镜像详情 | 替换 `AppState.image_detail` → 镜像抽屉 |
| `ImageLayers(Vec<(String, usize)>)` | 镜像层数批量回填 | 更新 `image_layers` 缓存 |
| `VolumeSizes(HashMap<String, u64>)` | 30s `/system/df` | 更新卷磁盘占用列 |

### 消息处理流水线

```
                     tokio spawn
  ┌──────────┐    refresh()     ┌──────────────┐    tx.send()    ┌───────────┐
  │ 主线程    │────────────────▶│ 后台 async    │──────────────▶│ Receiver  │
  │ UI 回调   │  /UI 操作/inspect│ bollard API   │                 │ (consumer)│
  │ /interval │                  │ follow_logs   │◀──Stream──────│           │
  └──────────┘                  └──────────────┘                 └─────┬─────┘
                                                                       │
                   ┌──────────────────────────────────────────────────┘
                   ▼
         ┌──────────────────────────┐
         │ lock state               │
         │ drain_messages ≤2048/批   │  逐条更新 containers / images / networks / volumes / detail / logs
         │ 整批裁剪 logs 到 500 行    │  （ui_update.rs，每条只做 O(1) 状态更新）
         │ drop lock                │
         └────────────┬─────────────┘
                      │ pending.merge（最多每 120ms 一次；无变化不 apply）
                      ▼
         ┌──────────────────────────┐
         │ lock state               │
         │ 只 build 变化区域          │  lists_changed → ui_rows::build_*_rows
         │ drop lock                │  drawer_changed → detail_fmt::build_drawer_data
         └────────────┬─────────────┘  logs_changed → logs.join("\n") + epoch
                      │ invoke_from_event_loop
                      ▼
         ┌──────────────────────────────────────────────────┐
         │ 主线程（闭包在 Slint 事件队列里执行）                 │
         │                                                    │
         │  render::containers(rows) 等 thread_local 缓存同步  │
         │    └ ui.set_container_rows / set_image_rows / ...  │
         │  epoch 匹配才 ui.set_log_text(log_text)             │
         │  ui_apply::apply_drawer(&ui, drawer)               │
         │  ui.set_total_* / set_running_containers / ...     │
         └──────────────────────────────────────────────────┘
```

### 日志流特殊点

`follow_logs` 返回 bollard 的 `impl Stream<Item=Result<LogOutput, Error>>`：

```rust
// LogOutput 是 struct 变体，不是 tuple！
// LogOutput::StdOut { message: Bytes } | LogOutput::StdErr { message: Bytes }

rt_handle.spawn(async move {
    let _ = docker::follow_logs(&id, tx, tail, epoch, cancel_rx).await;
    // 内部：select! { biased; cancel.changed() => break, item = stream.next() => ... }
    //       LogOutput::StdOut { message } → Bytes → String → tx.send(Msg::LogLine(epoch, line))
});
```

`AppState.follow_cancel` 持有 `watch::Sender`；切换容器时 `clear_logs()` drop sender，旧流的 `cancel.changed()` 立即返回 Err 并退出。工具条 Clear 走 `clear_log_buffer()`：有活动流时递增 log_epoch、drop 旧 sender，并以 `tail=0` 重启；无活动流时只清缓冲并递增 log_epoch。仅当确有缓冲或有活动流时把 `history_done` 置 true；空缓冲且无流时保留 false，切回日志 Tab 仍可用 tail=200 补拉启动日志。重新选中容器会重置 history_done。

流自然结束（容器停止/Docker 异常）时任务发送 `Msg::LogStreamEnded(epoch)`；consumer 仅在 stream_epoch 仍为当前流时清空 `follow_id` 并释放 sender。缓冲仍保留，下一次切到日志 Tab 以 `tail=0` 续流，不重放历史。空历史或首流失败时 `history_done` 仍为 false，会再次以 `tail=200` 重试。

**日志流自愈**（`src/follow_runtime.rs`，从 `AppState` 拆出以满足 R2 行数约束）：

`follow_logs` 返回 `(LogStreamEnd, lines)`，`classify_end` 判定可恢复性：

- `Cancelled`（切容器 / Clear）：立即结束，不重连。
- `Eof` + 容器已停止：结束，由下一次 `ensure_follow` 决定是否重建。
- `Eof` + 容器仍在运行（`AppState.follow_running` 随 3s 列表刷新更新）：视为可恢复，按退避重连——Docker 侧「干净关闭」不应让运行中容器的日志静默停摆。
- `Error` + 不可恢复（`No such container` / 404 等，容器被删除）：结束，避免僵尸任务无限重试。
- `Error` + 可恢复：按 `retry_delay` 退避重连，1s → 2s → 4s → 8s → 10s（上限），最多 `MAX_RETRY_ATTEMPTS = 8` 次后放弃并结束流；退避等待可被取消；重连沿用同一 stream epoch，期间不新建流。重连时若本流已推送过日志则改用 `tail=0`（只接续新行），否则沿用原 tail，避免重放历史造成重复行。

**UI 反馈**：首次失败发 `Msg::LogStreamStatus("Log stream interrupted, reconnecting…")`，恢复后发「已恢复连接」，放弃重连发「已停止」，均写入状态栏（i18n，不弹 toast）。

**流存活标志**：每条流持有独立的 `Arc<AtomicBool>`（`AppState.follow_active`）。`ensure_follow` 依据「id 相同 **且** 流仍存活」才幂等返回；旧流退出不会误标新流。这样 `LogStreamEnded` 已入队但尚未 drain 的窗口内切 Tab 也能正确重建流（不再出现「本次切 Tab 不建流、需再切一次」）。

> 已知限制：已收到部分历史后异常断流（daemon 重启/连接抖动），tail=0 从当前时刻接续，断流窗口内产生的日志不会补回；若需补齐，应使用 timestamps + `since` 游标，而不是回到 tail=200。

环形缓冲在 `drain_messages` 里整批裁剪（`drain(0..excess)`），避免逐行 `drain` 在洪峰下被放大；UI 侧只接收合并后的 `log_text`，不再逐行建 Model。

## 3. Rust 回调绑定三段式

```
callbacks.rs                    callbacks_container.rs      callbacks_image.rs
┌────────────────────┐          ┌───────────────────────┐   ┌──────────────────────┐
│ bind_callbacks()   │          │ bind_container_       │   │ bind_image_actions() │
│ {                  │          │   actions()            │   │ ┌─────────────────┐  │
│   bind_top_bar()   │          │ {                     │   │ │ on_image_remove │  │
│   bind_pull_dialog │          │ │ on_container_toggle │   │ └─────────────────┘  │
│   bind_cleanup_    │          │ │ on_container_restart│   └──────────────────────┘
│     dialog()       │          │ │ on_container_remove │
│   bind_container_  │          │ │ on_container_logs   │
│     actions() ←────┼──────────│ │   ├─ inspect (锁内) │
│   bind_image_      │          │ │   └─ follow_logs    │
│     actions() ←────┼──┐       │ │      (锁外 spawn)  │
│ }                  │  │       │ └────────────────────┘
└────────────────────┘  │       └───────────────────────┘
                        │
                        └─────────────────────────────────────────┐
                                                                  ▼
                                          app.slint global AppActions {
                                            callback container_toggle(string);
                                            callback container_restart(string);
                                            callback container_remove(string);
                                            callback container_logs(string);    ← 🆕
                                            callback image_remove(string);
                                          }
```

### 统一回调套路（铁律）

```rust
ui.on_xxx_requested(move || {
    // 1. 锁内取 handle + tx clone（微秒级）
    let (rt_handle, tx) = {
        let Ok(st) = s.lock() else { return };
        (st.rt.handle().clone(), st.tx.clone())
    };  // ← 立即 drop Mutex

    // 2. 锁外 spawn 真正的 Docker 操作
    rt_handle.spawn(async move {
        let result = docker::do_thing().await;
        let _ = tx.send(Msg::ActionDone(..., result));
    });
});
```

锁内**绝不**做任何可能耗时的操作（包括 await、invoke_from_event_loop）。

## 4. Rust 拆分原则

- 组装层分离：main.rs（i18n → AppState → MainWindow → 各模块启动 → ui.run() → 优雅退出）纯组装不写业务
- **consumer.rs 独占 Receiver + 分批 drain/节流 apply 两阶段泵**——3s 刷新 interval 也在 consumer.rs；6s stats、30s 卷占用与首屏加载在 bootstrap.rs
- **详情字段格式化抽成 detail_fmt/**——ui_state.rs 只管状态变更与后台任务
- **ui_update.rs 只做 Msg 聚合**——`DrainResult` + 有上限 `drain_messages` + 单条 `apply_msg`，保持日志洪峰防护逻辑独立
- **ui_state.rs 只做状态与后台任务**——刷新去重/超时、follow 建流、清理计数、配置持久化
- 回调按实体分：callbacks.rs（顶栏+弹窗+配置恢复）+ callbacks_container.rs + callbacks_image.rs
- 类型集中入口：`slint_gen.rs` 只做 `include_modules!()` 一次展开
- **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**——主线程不持有独立 tokio runtime
- **所有 Docker 操作结果都走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新
