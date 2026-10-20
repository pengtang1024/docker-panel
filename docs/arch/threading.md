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
// 每类列表独立标记：一条 stats 只重建容器行，不连带重建镜像/网络/卷
let container_rows = res.containers_changed.then(|| ui_rows::build_container_rows(&s));
let image_rows = res.images_changed.then(|| ui_rows::build_image_rows(&s));
let network_rows = res.networks_changed.then(|| ui_rows::build_network_rows(&s));
let volume_rows = res.volumes_changed.then(|| ui_rows::build_volume_rows(&s));
let drawer = res.drawer_changed.then(|| detail_fmt::build_drawer_data(&s));
// 日志行模型同步：模型是窗口的镜像，能追加就追加（可见行不重排 → 不卡）
let log_sync = res.logs_changed.then(|| {
    let epoch = s.log_epoch.load(Ordering::Acquire);
    let action = match s.log_sync_plan() {
        LogSyncPlan::Append { from } => LogModelSync::Append(rows_of(&s.logs[from..])),
        LogSyncPlan::Reset => LogModelSync::Reset(rows_of(&s.logs)),
    };
    s.commit_log_sync();
    (action, s.log_epoch.clone(), epoch)
});
drop(s);  // ⚠️ 必须在 invoke_from_event_loop 之前释放

invoke_from_event_loop(move || {
    // render::containers(rows) 走 thread_local 缓存模型，只 set 变化的列表
    ui_apply::apply_drawer(&ui, &drawer);
    // 构建增量后 UI 线程可能已 Clear/切容器：epoch 变了就丢弃旧行
    if let Some((action, epoch_ref, epoch)) = log_sync {
        if epoch_ref.load(Ordering::Acquire) == epoch {
            ui.set_log_rows(render::logs_sync(action));
        }
    }
}).ok();
```

关键点：

- **单批上限**：`MAX_DRAIN_BATCH = 2048`，避免日志洪峰时无界 `try_recv` 长时间占锁。
- **UI 节流**：`UI_MIN_INTERVAL = 120ms`（约 8 次/秒）；有待更新时 `poll_timeout` 只睡到窗口结束（≤100ms，兼顾 shutdown），把最坏 UI 应用延迟从约 200ms 收敛到约 120ms。
- **变化区域**：`DrainResult` 按列表分别标记 `containers_changed / images_changed / networks_changed / volumes_changed / drawer_changed / logs_changed`；纯日志消息不会重算容器/镜像/网络/卷/抽屉。`has_ui_changes()` 为 false 时不 apply（被 epoch 丢弃的旧日志不会触发空 invoke）。
- **日志缓冲**：`LogBatch(epoch, lines)` 整批推入 `AppState.logs`，超上限时**分块**裁剪（`LOG_ROWS_MAX = 2000` → 保留 `LOG_ROWS_KEEP = 1000`）；UI 侧是逐行的 `VecModel<LogRow>`，只追加不重建。
- **日志流取消**：`AppState` 持有 `watch::Sender`；切换容器时 drop sender，旧 `follow_logs` 在 `select!` 中立即退出。日志行携带 stream epoch，consumer 只接受当前流；工具条 Clear 以 `tail=0` 重启当前流，清空前的排队日志按旧 epoch 丢弃。
- **日志续流**：日志缓冲代际与流代际分离。`log_epoch` 只在 clear/切容器时递增，用于 UI 日志行模型的过期判断；`stream_epoch` 每次建流递增，用于日志行/结束消息。`history_done` 记录当前选中会话是否已加载历史：收到首条日志，或在有缓冲/活动流时 Clear 后置 true；空缓冲且无活动流时保持 false，让下次 `ensure_follow` 用 `tail=200` 重试。首次/空历史/首流失败用 `tail=200`，否则用 `tail=0`。重新选中容器（含同一容器）会重置，Clear 的持久性仅到下一次选中。
- **两阶段铁律**：锁内只 build，`invoke_from_event_loop` 闭包绝不碰 `Mutex<AppState>`；`log_epoch` 用共享 `Arc<AtomicU64>`，闭包只读原子判断行模型是否已过期。
- **行模型过期判定（`rows_epoch`）**：行是在锁内按**当时**的排序/筛选/搜索条件建的，锁外才写 UI；期间用户改了条件，这批行落地就会让列表顺序与排序箭头打架。排序/筛选/搜索变更时回调递增 `AppState::rows_epoch`，consumer 在锁内快照、锁外比对，epoch 变了就丢弃这批旧行（回调已用新条件重建并应用过）。
- **consumer 不回写用户操作过的状态**：`sort_field` / `sort_desc` / `filter` / `view` 等由 UI 回调独占写入（回调在 UI 线程同步改 state → `ui.set_xxx` → 重建列表）。consumer 只刷数据：它锁内取的是快照、锁外才写 UI，回写就会把用户刚改的状态打回旧值（箭头滞后最多 3s，甚至偶发对不上）。详见 [ui-interaction.md §3.2](ui-interaction.md)。

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
| **`StatsBatch(Vec<(String, ContainerStatsSnapshot)>)`** | 6s stats interval | 写入 `stats_prev` 双采样缓冲 → CPU%/内存/网络列（整轮打包成一条消息，卡片数值同帧更新） |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 关弹窗 + 标记立即刷新 |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本 + 标记立即刷新 |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 标记立即刷新 |
| **`ContainerDetail(Result<ContainerDetail>)`** | `inspect_container` → 详情面板 | 替换 `AppState.detail` → Drawer 详情 Tab |
| **`LogBatch(u64, Vec<String>)`** | `follow_logs` Stream | `(stream_epoch, 行列表)`；不匹配当前流则丢弃 → 推入 `AppState.logs`（窗口 ≤2000 行）→ 追加到 UI 行模型 |
| `LogStreamEnded(u64)` | follow_logs 任务结束 | 仅当前 stream_epoch 时清空 `follow_id` 并释放 cancel sender，允许下次重新建流 |
| **`LogStreamStatus(u64, String)`** | follow_runtime 判定结局 | 仅当前 stream_epoch 时写入 `log_status`（日志面板工具栏状态位）；文案见 `src/log_status.rs`（重连中 / 已恢复 / 已停止） |
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
                   ┌─────────────────────────────────────────────────┘
                   ▼
         ┌──────────────────────────┐
         │ lock state               │
         │ drain_messages ≤2048/批   │  逐条更新 containers / images / networks / volumes / detail / logs
         │ 分块裁剪 logs 到 ≤2000 行  │  （ui_update.rs，每条只做 O(1) 状态更新）
         │ drop lock                │
         └────────────┬─────────────┘
                      │ pending.merge（最多每 120ms 一次；无变化不 apply）
                      ▼
         ┌──────────────────────────┐
         │ lock state               │
         │ 只 build 变化区域          │  containers_changed → ui_rows::build_container_rows
         │ drop lock                │  drawer_changed → detail_fmt::build_drawer_data
         └────────────┬─────────────┘  logs_changed → log_sync_plan() → Append / Reset
                      │ invoke_from_event_loop
                      ▼
         ┌──────────────────────────────────────────────────┐
         │ 主线程（闭包在 Slint 事件队列里执行）                 │
         │                                                    │
         │  render::containers(rows) 等 thread_local 缓存同步  │
         │    └ ui.set_container_rows / set_image_rows / ...  │
         │  epoch 匹配才 ui.set_log_rows(render::logs_sync(..)) │
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
    //       LogOutput::StdOut { message } → Bytes → String → 攒进行批
    //       攒够 LOG_BATCH_MAX(1024) 行或距首行 LOG_FLUSH_INTERVAL(60ms) → tx.send(Msg::LogBatch(epoch, lines))
});
```

**行批投递（洪峰铁律）**：日志洪峰可达 4 万+ 行/秒。逐行 `tx.send` 会让 mpsc 每行唤醒一次 consumer（惊群），实测 CPU 打到 240%+。因此 `follow_logs` 只做三件事：① 行先攒进 `batch`，满 `LOG_BATCH_MAX = 1024` 行或距首行 `LOG_FLUSH_INTERVAL = 60ms` 才发一条 `Msg::LogBatch`（通道消息数下降 3 个数量级）；② 空闲超时用一个**常驻** `Sleep`（每行只 `reset` 截止时间，不逐行新建 `timeout`）；③ 每行只做一次 UTF-8 校验 + 一次堆分配。

`AppState.follow_cancel` 持有 `watch::Sender`；切换容器时 `clear_logs()` drop sender，旧流的 `cancel.changed()` 立即返回 Err 并退出。工具条 Clear 走 `clear_log_buffer()`：有活动流时递增 log_epoch、drop 旧 sender，并以 `tail=0` 重启；无活动流时只清缓冲并递增 log_epoch。仅当确有缓冲或有活动流时把 `history_done` 置 true；空缓冲且无流时保留 false，切回日志 Tab 仍可用 tail=200 补拉启动日志。重新选中容器会重置 history_done。

流自然结束（容器停止/Docker 异常）时任务发送 `Msg::LogStreamEnded(epoch)`；consumer 仅在 stream_epoch 仍为当前流时清空 `follow_id` 并释放 sender。缓冲仍保留，下一次切到日志 Tab 以 `tail=0` 续流，不重放历史。空历史或首流失败时 `history_done` 仍为 false，会再次以 `tail=200` 重试。

**日志流自愈**（`src/follow_runtime.rs`，从 `AppState` 拆出以满足 R2 行数约束）：

`follow_logs` 返回 `LogStreamOutcome { end, lines, notified_recovered }`。**容器是否还活着由探针判定**：`probe_container` → `ContainerReach { Running, Stopped, Gone, Unreachable }`（`src/docker/container_detail.rs`），`classify_end(end, reach)` 据此得出 `FollowAction`：

- `Cancelled`（切容器 / Clear）：`End`，不重连、不提示。
- `Eof` + 探针 `Stopped` / `Gone`：容器已停止或被删除，日志流本就该结束 → `End`。
- `Eof` + 探针 `Running`：容器还在跑却 EOF → `Recover`，按退避重连（Docker 侧「干净关闭」不应让运行中容器的日志静默停摆）。
- `Eof` + 探针 `Unreachable`：Docker 整体不可达 → `Recover`，等它回来。
- `Idle`（超过 `LOG_IDLE_TIMEOUT = 180s` 无任何日志行）：连接可能挂死 → `KeepAlive`，**静默**重连（`IDLE_RETRY_DELAY = 1s`），不打扰用户、**不计入失败预算**（这是长连接健康检查，不是失败）。
- `Error` + 致命（`LogError.status_code == 404` / `No such container`，容器已被删除）：`End`，避免僵尸任务无限重试。
- `Error` + 可恢复：`Recover`，按 `retry_delay` 退避重连，1s → 2s → 4s → 8s → 10s（上限）。**连续失败预算 `MAX_RETRY_ATTEMPTS = 8` 耗尽后转 30s 低频保活（`KEEPALIVE_INTERVAL`），永不放弃跟随**——旧实现会静默停止，用户看到日志停在半截却毫无提示。退避等待可被取消；重连沿用同一 stream epoch，期间不新建流。重连时若本流已推送过日志则改用 `tail=0`（只接续新行），否则沿用原 tail，避免重放历史造成重复行。

**UI 反馈**：日志流状态走**独立通道** `Msg::LogStreamStatus(epoch, text)`，写入日志面板工具栏的状态位（`ui/app.slint` 的 `log_status` → `Drawer.log_status`），不占用底部状态栏，也不受操作 toast 的 2s「就绪」定时器影响；带 epoch 过滤，旧流的提示不会落到新容器。文案唯一定义处是 `src/log_status.rs` 的 3 条 i18n key 常量：

| 提示 | 触发时机 |
|---|---|
| 「日志流中断，正在重连…」 | 进入 `Recover` 且此前未提示过（每条流只提示一次，不刷屏） |
| 「日志流已恢复连接」 | ① 重连后收到**首行日志**；② `Idle` 结局（连接已保持 180s 证明流通）；③ 探针从「不可达」跃迁到「可达」。三者均要求此前已提示过「正在重连」 |
| 「日志流已停止」 | `End` 且非 `Cancelled`（容器停止 / 被删除 / 致命错误） |

**状态清除**：切换选中资源或按 Clear 走 `ui_apply::clear_log_panel`（日志行模型 + 状态提示全清）；同一容器重建日志流走 `clear_log_status`（只清状态、保留已缓冲的日志行）——新流没有「正在重连」之类的历史状态，但旧内容仍有效。`AppState::ensure_follow` 返回「是否新建了流」，调用方据此决定是否清残留提示。

**流存活标志**：每条流持有独立的 `Arc<AtomicBool>`（`AppState.follow_active`）。`ensure_follow` 依据「id 相同 **且** 流仍存活」才幂等返回；旧流退出不会误标新流。这样 `LogStreamEnded` 已入队但尚未 drain 的窗口内切 Tab 也能正确重建流（不再出现「本次切 Tab 不建流、需再切一次」）。跨流共享的 running 标志已移除——容器存活一律以探针结论为准。

### 日志行模型：只追加，不重建（`src/ui_state/log_sync.rs`）

日志面板用**虚拟化 `ListView`**（逐行 `LogLineRow` delegate），只实例化视口内的行。但虚拟化只解决「渲染多少行」，不解决「模型怎么变」——如果每批日志都把模型整体替换，可见行会被标记为脏并重新排版，卡顿依旧（实测 500 行缓冲时软件渲染器吃掉一个 CPU 核）。

因此行模型被设计成**日志窗口的镜像**，两者用「全局行流位置」对齐：

| 位置 | 含义 |
|------|------|
| 窗口 = `[logs_total - logs.len(), logs_total)` | `AppState.logs`（滚动窗口，≤ `LOG_ROWS_MAX = 2000` 行） |
| 模型 = `[model_total - model_rows, model_total)` | UI 侧 `VecModel<LogRow>` |

- **起点相同** → 模型是窗口的前缀 → `LogSyncPlan::Append { from }`：只 `extend` 新行（1 个 `RowAdded` 事件），已有行索引不变，可见行不重排 → **不卡**。
- **起点不同** → 窗口裁剪过前段（或换了流）→ `LogSyncPlan::Reset`：`set_vec` 整体替换（1 个 reset 事件）。分块裁剪（`LOG_ROWS_KEEP = 1000`）保证这一步每 1000 行才发生一次。

裁剪必须**分块**而不是逐行丢最旧行：逐行 `drain(0..1)` 会让整表索引每行错位一次，Slint 的 ListView 每次都把可见行标脏重排——这正是旧实现卡顿的主因。

> 已知限制：已收到部分历史后异常断流（daemon 重启/连接抖动），tail=0 从当前时刻接续，断流窗口内产生的日志不会补回；若需补齐，应使用 timestamps + `since` 游标，而不是回到 tail=200。

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
- **日志窗口裁剪与行模型同步账归 ui_state/log_sync.rs**——`LogSyncPlan` / `trim_logs` / `log_sync_plan` / `commit_log_sync` 与其单测独立成模块，ui_state.rs 保持 ≤300 行
- 回调按实体分：callbacks.rs（顶栏+弹窗+配置恢复）+ callbacks_container.rs + callbacks_image.rs
- 类型集中入口：`slint_gen.rs` 只做 `include_modules!()` 一次展开
- **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**——主线程不持有独立 tokio runtime
- **所有 Docker 操作结果都走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新
