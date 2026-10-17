# 运行时架构：线程模型 / 消息流 / 回调绑定

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是线程模型、消息流与回调纪律的权威深度版。

## 1. 线程模型（三线程架构）

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ UI 回调（搜索/排序/    │       │ recv_timeout(200ms) 循环 │       │ 3s 刷新 interval    │
│  操作/快捷键）──lock─▶│ drain_messages            │       │ 6s stats interval   │
│ 窗口关闭 → shutdown   │ build_*_rows ×4           │       │ spawn 异步任务      │
│   flag                │ detail_fmt::build_detail  │◀──────│ bollard API 调用    │
│                        │ drop(s) ──────────────────lock rel│ tx.send(Msg)        │
│                        │ invoke_from_event_loop     │       │ follow_logs 流      │
│                        │   apply_lists ×5 模型      │       │ inspect_container   │
│                        │   apply_detail / counts    │       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
   + shutdown: AtomicBool            （consumer 独占，不进 Mutex）
```

### Receiver 不进 Mutex — 死锁唯一保证

`mpsc::Receiver<Msg>` 由 consumer 线程独占所有权，**不**放在 `AppState` 里。
这样 `rx.recv_timeout(200ms)` 永远不会阻塞主线程拿 state 锁。
如果 Receiver 放进 Mutex，consumer 阻塞 `recv()` 时持有锁 → 主线程任何 `state.lock()`（UI 回调、interval tick）全卡死。

### pump_and_apply 两阶段铁律（consumer.rs）

```rust
// Phase 1 — 锁内（微秒级）：
let Ok(mut s) = state.lock() else { return };
let (need_update, latest_msg) = s.drain_messages(rx, first);
let (cm, im, nm, vm) = (build_container_rows(&s), build_image_rows(&s),
                        build_network_rows(&s), build_volume_rows(&s));
let detail_data = detail_fmt::build_detail_data(&s);        // 18 字段 SharedString 元组
let image_detail_data = detail_fmt::build_image_detail_data(&s);
let log_lines: Vec<LogLineRow> = s.logs.iter().map(...).collect();
// 操作完成立即刷新（ActionDone / PullDone / CleanupDone）→ 锁内 s.refresh()
drop(s);  // ⚠️ 必须在 invoke_from_event_loop 之前释放

// Phase 2 — 锁外：
invoke_from_event_loop(move || {
    let Some(ui) = ui_w.upgrade() else { return };
    ui_apply::apply_lists(&ui, cm, im, nm, vm, lm);   // 5 个 ModelRc 一次灌入
    // ... counts / sort_field / status_text / docker_ok / pull_open ...
    ui_apply::apply_detail(&ui, &detail_data);        // None 时无操作
    ui_apply::apply_image_detail(&ui, &image_detail_data);
}).ok();
```

### 定时刷新走 tokio interval（不是 Slint Timer）

软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），定时任务全部改为 tokio interval：

- **3s 列表刷新**（`consumer::spawn_refresh_interval`）：锁内 clone handle → drop → spawn `interval(REFRESH_INTERVAL)` 循环，每 tick `s.refresh()` 发起容器/镜像列表任务，结果走 `tx.send(Msg::*)`。网络/卷不在 3s 周期内，由操作后刷新与搜索/排序回调即时重建（`ui_apply::refresh_lists_now`）。
- **6s stats 采样**（`bootstrap::spawn_stats_interval`）：join_all 拉所有 running 容器 stats，首帧跳过——双采样 CPU% 算法需要前一帧。
- UI 回调与 interval tick 本身**不**碰 Slint Model——只发任务/改 state，consumer 统一调用 `invoke_from_event_loop` 更新 UI。
- 这符合 Slint 线程安全模型：只有主线程可以修改 Model。

## 2. 消息流（Msg 变体一览）

| 变体 | 触发源 | UI 行为 |
|------|--------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | 3s interval / 手动刷新 | 替换容器列表 + 剪失效选择 |
| `Images(Result<Vec<ImageInfo>>)` | 3s interval / 手动刷新 | 替换镜像列表 + 剪失效选择 |
| **`Networks(Result<Vec<NetworkInfo>>)`** | 3s interval / 手动刷新 / 操作后刷新 | 替换网络列表（网络视图） |
| **`Volumes(Result<Vec<VolumeInfo>>)`** | 3s interval / 手动刷新 / 操作后刷新 | 替换卷列表（卷视图） |
| **`Stats(String id, ContainerStatsSnapshot)`** | 6s stats interval | 写入 `stats_prev` 双采样缓冲 → CPU%/内存/网络列 |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 关弹窗 + **锁内立即 `s.refresh()`** |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本 + **锁内立即 `s.refresh()`** |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + **锁内立即 `s.refresh()`** |
| **`ContainerDetail(Result<ContainerDetail>)`** | `inspect_container` → 详情面板 | 替换 `AppState.detail`，pump_and_apply 灌入 Drawer 详情 Tab |
| **`LogLine(String)`** | `follow_logs` Stream | 推入 `AppState.logs`（环形缓冲 cap=500），pump_and_apply 灌入日志 Tab |

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
         ┌────────────────┐
         │ lock state      │
         │ drain_messages  │  逐条更新 containers / images / networks / volumes / detail / logs
         │ build rows ×4   │  （ui_rows::build_*_rows + detail_fmt::build_detail_data）
         │ drop lock       │
         └────────┬───────┘
                  │ invoke_from_event_loop
                  ▼
         ┌──────────────────────────────────────────────────┐
         │ 主线程（闭包在 Slint 事件队列里执行）                 │
         │                                                    │
         │  ui_apply::apply_lists(&ui, cm, im, nm, vm, lm)   │
         │    └ ui.set_container_rows(ModelRc::from(...))     │
         │    └ ui.set_image_rows / set_network_rows / ...    │
         │    └ ui.set_log_lines(ModelRc::from(...))         │
         │                                                    │
         │  if let Some(ref d) = detail_data:                 │
         │    ui_apply::apply_detail(&ui, d)                  │
         │      └ ui.set_d_name / set_d_state / set_d_... ×18│
         │                                                    │
         │  ui.set_total_* / set_running_containers / ...    │
         └──────────────────────────────────────────────────┘
```

### 日志流特殊点

`follow_logs` 返回 bollard 的 `impl Stream<Item=Result<LogOutput, Error>>`：

```rust
// LogOutput 是 struct 变体，不是 tuple！
// LogOutput::StdOut { message: Bytes } | LogOutput::StdErr { message: Bytes }

rt_handle.spawn(async move {
    let _ = docker::follow_logs(&id, tx, 200).await;
    // 内部：stream.next().await → LogOutput::StdOut { message } → Bytes → String → tx.send(Msg::LogLine(line))
});
```

环形缓冲在 `drain_messages` 里做（`drain(0..excess)`），避免 `push` 无限增长。

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
- **consumer.rs 独占 Receiver + pump_and_apply 两阶段泵**——3s 刷新 interval 也在 consumer.rs；6s stats 与首屏加载在 bootstrap.rs
- **详情字段格式化抽成 detail_fmt.rs**——ui_state.rs 只管状态变更与消息排空
- **derive_update match 抽成独立 ui_update.rs**——update 推导单独一个文件
- 回调按实体分：callbacks.rs（顶栏+弹窗+配置恢复）+ callbacks_container.rs + callbacks_image.rs
- 类型集中入口：`slint_gen.rs` 只做 `include_modules!()` 一次展开
- **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**——主线程不持有独立 tokio runtime
- **所有 Docker 操作结果都走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新
