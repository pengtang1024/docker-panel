# 运行时架构：线程模型 / 消息流 / 回调绑定

> **一句话**：线程怎么分、消息怎么流、回调怎么写才不会死锁——改 `consumer` / 回调 / 日志流之前先读这一页。

**对应源码**：`src/consumer.rs`、`src/ui_update.rs`、`src/ui_state.rs`、`src/callbacks.rs`、`src/follow_runtime.rs`｜**相关页面**：[架构总纲](../architecture.md)、[UI 层架构](ui-interaction.md)

## 1. 线程模型（三线程架构）

**结论：主线程只跑 UI，consumer 线程独占 Receiver 做批量泵，tokio runtime 承担全部 Docker 调用与定时任务；三者只通过 `Mutex<AppState>` 与 `mpsc` 通信。**

```text
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ UI 回调（搜索/排序/    │       │ recv_timeout(≤100ms) 循环 │       │ docker /events 订阅 │
│  操作/快捷键）──lock─▶│ 分批 drain（≤2048 条/批） │       │ 6s stats interval   │
│ 窗口关闭 → shutdown   │ pending.merge 聚合        │       │ 30s 磁盘占用 interval│
│   flag                │ 120ms 节流 apply          │◀──────│ bollard API 调用    │
│                        │ 只重建变化区域 → drop lock│       │ tx.send(Msg)        │
│                        │ invoke_from_event_loop     │       │ follow_logs 流      │
│                        │   render 缓存 + apply_drawer│      │ inspect_container   │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
   + shutdown: AtomicBool            （consumer 独占，不进 Mutex）
```

### Receiver 不进 Mutex — 死锁唯一保证

`mpsc::Receiver<Msg>` 由 consumer 线程独占所有权，**不**放在 `AppState` 里；这样 `rx.recv_timeout(≤100ms)` 永远不会阻塞主线程拿 state 锁。

> [!WARNING]
> 把 Receiver 放进 Mutex，consumer 阻塞 `recv()` 时就会持有锁 → 主线程任何 `state.lock()`（UI 回调、interval tick）全部卡死。这是本项目唯一需要防的死锁来源。

### 分批 drain + 120ms 节流（consumer.rs）

日志洪峰下，「每条 Msg 一次 invoke + 重建全部列表」会把 UI 线程压死，也会因长时间持锁饿死点击回调；当前实现分三层防护。

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
let container_ledger = res.containers_changed.then(|| ui_rows::build_container_ledger(&s));
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

关键点（每条都是维护时的硬约束）：

| 要点 | 纪律 |
|------|------|
| 单批上限 | `MAX_DRAIN_BATCH = 2048`，避免日志洪峰时无界 `try_recv` 长时间占锁 |
| UI 节流 | `UI_MIN_INTERVAL = 120ms`（约 8 次/秒）；有待更新时 `poll_timeout` 只睡到窗口结束（≤100ms，兼顾 shutdown），把最坏 UI 应用延迟从约 200ms 收敛到约 120ms |
| 变化区域 | `DrainResult` 按列表分别标记 `containers_changed / images_changed / networks_changed / volumes_changed / drawer_changed / logs_changed`；纯日志消息不会重算容器/镜像/网络/卷/抽屉。`has_ui_changes()` 为 false 时不 apply（被 epoch 丢弃的旧日志不会触发空 invoke） |
| 日志缓冲 | `LogBatch(epoch, lines)` 整批推入 `AppState.log.logs`，超上限时**分块**裁剪（`LOG_ROWS_MAX = 2000` → 保留 `LOG_ROWS_KEEP = 1000`）；UI 侧是逐行的 `VecModel<LogRow>`，只追加不重建 |
| 日志流取消 | `AppState` 持有 `watch::Sender`；切换容器时 drop sender，旧 `follow_logs` 在 `select!` 中立即退出。日志行携带 stream epoch，consumer 只接受当前流；工具条 Clear 以 `tail=0` 重启当前流，清空前的排队日志按旧 epoch 丢弃 |
| 日志续流 | 缓冲代际与流代际分离：`log_epoch` 只在 clear/切容器时递增（UI 行模型过期判断），`stream_epoch` 每次建流递增（日志行/结束消息）。`history_done` 记录当前选中会话是否已加载历史：收到首条日志，或在有缓冲/活动流时 Clear 后置 true；空缓冲且无活动流时保持 false，让下次 `ensure_follow` 用 `tail=200` 重试。首次/空历史/首流失败用 `tail=200`，否则用 `tail=0`。重新选中容器（含同一容器）会重置，Clear 的持久性仅到下一次选中 |
| 两阶段铁律 | 锁内只 build，`invoke_from_event_loop` 闭包绝不碰 `Mutex<AppState>`；`log_epoch` 用共享 `Arc<AtomicU64>`，闭包只读原子判断行模型是否已过期 |
| 行模型过期判定 | `rows_epoch`（per-list 位掩码）：行在锁内按**当时**的排序/筛选/搜索条件建，锁外才写 UI；期间用户改了条件，这批行落地就会让列表顺序与排序箭头打架。排序/筛选/搜索变更时回调按受影响列表递增 `AppState::rows_epoch`（搜索 = 容器 + 镜像；排序 = 对应列表），consumer 锁内快照、锁外逐列表比对，该列表代际变了才丢弃旧行（回调已用新条件重建并应用过） |
| consumer 不回写 | `sort_field` / `sort_desc` / `filter` / `view` 等由 UI 回调独占写入（回调在 UI 线程同步改 state → `ui.set_xxx` → 重建列表）；consumer 只刷数据，锁内取快照、锁外写 UI，回写会把用户刚改的状态打回旧值。详见 [ui-interaction.md](ui-interaction.md) 的「UI 状态单一写入方」 |

### 定时任务走 tokio interval（不是 Slint Timer）

软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），所有定时任务改为 tokio interval。

| 任务 | 触发 / 频率 | 纪律 |
|------|-------------|------|
| 列表刷新 | `event_watch::spawn` 常驻订阅 Docker `/events` | **只有事件这一个来源**：`type` 过滤 container/image/network/volume，动作过滤用**噪声黑名单**（只忽略 `health_status` / `exec_*` / `attach` / `detach`，未知动作宁可多刷一次也不漏刷）；收到非噪声事件即锁内 `s.refresh_kinds(kinds_of(&ev))`——**按事件类型只刷那一路**（`Kinds` 位掩码；容器事件只拉 `/containers/json`，实测 18ms vs 全量 77.7ms，镜像的「被引用容器数」改用新容器列表在应用侧本地重算，网络/卷只在自身事件或全量刷新时拉；连接建立 / 手动刷新 / prune 兜底仍走 `Kinds::ALL` 四路并发），结果走 `tx.send(Msg::*)`→ 400ms 合并窗口**双沿**：窗口内首事件立即刷新（动作反馈快），后续事件只记账、到期补刷一次（`Debouncer`，只丢不补会漏掉窗口尾部的状态变化）；断流按 1s→30s 指数退避重连、永不放弃，重连后**补一次全量刷新**（连接建立那一刻的状态才是真相）；空闲时零请求。**没有定时兜底轮询**：曾用 3s（`size=1` 容器列表 ≈70ms + 镜像列表 ≈96ms，每分钟吃掉 dockerd ≈3.3s），15s 兜底也一起删掉了——兜底只会把「事件漏刷 / 过滤写错 / 代理不支持长连接」这类 bug 藏成「最多慢 N 秒」。`refresh_kinds()` 带 in-flight 去重（上一轮未完成只把种类并进 `refresh_dirty` 掩码、本轮结束按掩码补跑，不丢事件触发的刷新，也不会在慢 Docker 下叠多个调用） |
| 6s stats 采样 | `bootstrap::spawn_stats_interval` | join_all 拉所有 running 容器 stats，且**一律 `one_shot(true)`**（不带 pre 帧、不等 1–2s 双采样，整轮 2.6–3ms）。**首轮不跳过**，且**首屏拿到容器列表后立刻补采一轮**（`spawn_initial_load`）→ 启动后约 1s 就有 CPU / 内存 / 网络，而不是等 6s + 6s；同一函数独立 spawn 一次 `/version` 查询（`Msg::DockerVersion`），慢或失败都不阻塞首屏四列表。CPU% 靠差值计算，本地无 prev（进程刚起 / 首次采样）时**用 Docker 同帧返回的 `precpu_stats` 兜底**（仅首屏那一轮走双采样，之后靠本地 prev 做 6s 窗口差值，见 [docker-bollard.md](docker-bollard.md)）→ 首帧即真实 CPU%，不会先显示一轮 0.0% |
| 30s 磁盘占用 | `bootstrap::spawn_disk_usage_interval` | `/system/df?type=volume` 与 `?type=container` 并行采样，低频单独跑。容器可写层占用只从这里来（列表接口不带 `size=1`：遍历文件系统 + 删除中的容器会 404），列表刷新后用 `AppState::apply_container_sizes` 从缓存补回 |
| 自动更新 | `update::spawn_scheduler` | 启动 10s 后检查一次，之后每 6h 醒来判断是否满 24h。`spawn_check` / `spawn_install` 内的网络（系统 `curl`）、`pkexec`、文件 IO 全部放 `tokio::task::spawn_blocking`；进度按 1% 节流发 `Msg::UpdateProgress`；检查与安装共用 `AppState.update.update_busy` 互斥，重复点击不会叠加任务。安装前的 dpkg / 目录可写 / pkexec 判定与下载后的 SHA-256 校验见 `src/update/install.rs` |
| UI 回调与 tick | 主线程 | 本身**不**碰 Slint Model——只发任务/改 state，consumer 统一调用 `invoke_from_event_loop` 更新 UI（Slint 线程安全模型：只有主线程可以修改 Model） |
| 主题监听 | `dconf watch` 独立 reader 线程 | 阻塞读放在独立 reader 线程，监控线程用 `recv_timeout(200ms)` 轮询 shutdown；轮询回退路径也按 200ms 醒来检查。窗口关闭时 `theme-monitor` 能在 200ms 内 join，不会因 `reader.next()` 永久阻塞而让进程挂住 |

所有列表 / 统计 / inspect 调用都经 `docker::with_timeout`（10s），防止守护进程无响应时后台任务无限挂起、耗尽 tokio 工作线程。

## 2. 消息流（Msg 变体一览）

**结论：所有后台结果只走一条通道——`tx.send(Msg)`；`Msg` 是 Rust → UI 的唯一数据契约，consumer 是唯一消费者。**

| 变体 | 触发源 | UI 行为 |
|------|--------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | `/events` / 手动刷新 / 操作完成 | 替换容器列表（保留旧 stats） |
| `Images(Result<Vec<ImageInfo>>)` | `/events` / 操作完成 | 替换镜像列表 + 后台补查层数 |
| `Networks(Result<Vec<NetworkInfo>>)` | `/events` / 手动刷新 / 操作完成 | 替换网络列表（网络视图） |
| `Volumes(Result<Vec<VolumeInfo>>)` | `/events` / 手动刷新 / 操作完成 | 替换卷列表（卷视图） |
| **`DockerVersion(Result<String>)`** | 启动时独立任务查询 `/version`（与首屏四列表并行，慢/失败不拖列表） | 写入 `DrainResult.docker_version`（底栏版本文案：`v{版本}`；取不到为占位符 `—`，不伪造版本号） |
| **`StatsBatch(Vec<(String, ContainerStatsSnapshot)>)`** | 6s stats interval + 首屏加载补采一轮 | 写入 `stats_prev` 双采样缓冲 → CPU%/内存/网络列（整轮打包成一条消息，容器行数值同帧更新）；本地无 prev 时用 `precpu_stats` 兜底，首帧即真实 CPU% |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 关弹窗；列表交给 `image/pull` 事件（不刷新） |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本；列表交给 `/events`（不刷新） |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 标记 `refresh_now`（prune 兜底） |
| **`ContainerDetail(Result<ContainerDetail>)`** | `inspect_container` → 详情面板 | 替换 `AppState.detail` → Drawer 详情 Tab |
| **`LogBatch(u64, Vec<String>)`** | `follow_logs` Stream | `(stream_epoch, 行列表)`；不匹配当前流则丢弃 → 推入 `AppState.log.logs`（窗口 ≤2000 行）→ 追加到 UI 行模型 |
| `LogStreamEnded(u64)` | follow_logs 任务结束 | 仅当前 stream_epoch **且全部流都退出**（`follow_is_alive()` 为 false）时清空 `follow_id` / `follow_targets` / `follow_active` 并释放 cancel sender，允许下次重新建流 |
| **`LogStreamStatus(u64, String)`** | follow_runtime 判定结局 | 仅当前 stream_epoch 时写入 `log_status`（日志面板工具栏状态位）；文案见 `src/log_status.rs`（重连中 / 已恢复 / 已停止 / 洪峰抽样，共 4 条） |
| `ImageDetail(Result<ImageDetail>)` | `inspect_image` → 镜像详情 | 替换 `AppState.image_detail` → 镜像抽屉 |
| `ImageLayers(Vec<(String, usize)>)` | 镜像层数批量回填 | 更新 `image_layers` 缓存 |
| `VolumeSizes(HashMap<String, u64>)` | 30s `/system/df` | 更新卷磁盘占用列 |
| **`UpdateCheckDone(bool, bool, Result<UpdateInfo, String>)`** | 启动 10s 后 / 每 6h 轮询（24h 节流）/ 手动检查 | `(manual, auto_installing, result)`；成功写入 `AppState.update.update_info` 并更新顶栏徽标；启动检查失败静默，手动失败写弹窗状态 |
| **`UpdateProgress(i32, String)`** | 下载 / 安装任务 | 弹窗进度条 + 状态文案（1% 节流，不逐 chunk 发） |
| **`UpdateDone(Result<String, String>, bool)`** | 安装任务结束 | 成功标记 `update_restart_needed`、顶栏徽标转琥珀色；**`auto_restart` 开启时置 `restart_on_exit`、3s 后仅退出事件循环**（`schedule_auto_restart`，`src/ui_update/update.rs:100-102`）——真正拉起新二进制在 `main` 的退出流程里：先 `single_instance::release_then` 释放锁，再 `update::install::restart_detached` 起 launcher（`sleep 2; exec <exe>`）。**顺序不能反**：先拉起后退出会让新进程撞上旧锁、判 `AlreadyRunning` 退出，旧进程随后也退出 → 应用消失；失败保留可用状态供重试 |

### 消息处理流水线

```text
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
         │ drain_messages ≤2048/批   │  逐条更新 containers / images / networks / volumes / detail / logs / docker_version
         │ 分块裁剪 logs 到 ≤2000 行  │  （ui_update.rs，每条只做 O(1) 状态更新）
         │ drop lock                │
         └────────────┬─────────────┘
                      │ pending.merge（最多每 120ms 一次；无变化不 apply）
                      ▼
         ┌──────────────────────────┐
         │ lock state               │
         │ 只 build 变化区域          │  containers_changed → ui_rows::build_container_ledger
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

**行批投递（洪峰铁律）**：日志洪峰可达 4 万+ 行/秒，逐行 `tx.send` 会让 mpsc 每行唤醒一次 consumer（惊群），实测 CPU 打到 240%+。因此 `follow_logs` 只做三件事：

| # | 手段 | 做法 |
|---|------|------|
| ① | 攒批 | 行先攒进 `batch`，满 `LOG_BATCH_MAX = 1024` 行或距首行 `LOG_FLUSH_INTERVAL = 60ms` 才发一条 `Msg::LogBatch`（通道消息数下降 3 个数量级） |
| ② | 常驻定时器 | 空闲超时用一个**常驻** `Sleep`（每行只 `reset` 截止时间，不逐行新建 `timeout`） |
| ③ | 少做转换 | 每行只做一次 UTF-8 校验 + 一次堆分配 |

`AppState.log.follow_cancel` 持有 `watch::Sender`；切换容器时 `clear_logs()` drop sender，旧流的 `cancel.changed()` 立即返回 Err 并退出。工具条 Clear 走 `clear_log_buffer()`：有活动流时递增 log_epoch、drop 旧 sender，并以 `tail=0` 重启；无活动流时只清缓冲并递增 log_epoch。仅当确有缓冲或有活动流时把 `history_done` 置 true；空缓冲且无流时保留 false，切回日志 Tab 仍可用 tail=200 补拉启动日志。重新选中容器会重置 history_done。

流自然结束（容器停止/Docker 异常）时任务发送 `Msg::LogStreamEnded(epoch)`；consumer 仅在 stream_epoch 仍为当前流**且全部流都退出**（`follow_is_alive()` 为 false；分组合并日志有 N 条流）时清空 `follow_id` / `follow_targets` / `follow_active` 并释放 sender。缓冲仍保留，下一次切到日志 Tab 以 `tail=0` 续流，不重放历史。空历史或首流失败时 `history_done` 仍为 false，会再次以 `tail=200` 重试。

**日志流自愈**（`src/follow_runtime.rs`，从 `AppState` 拆出以满足 R2 行数约束）：`follow_logs` 返回 `LogStreamOutcome { end, lines, notified_recovered }`；**容器是否还活着由探针判定**——`probe_container` → `ContainerReach { Running, Stopped, Gone, Unreachable }`（`src/docker/container_detail.rs`），`classify_end(end, reach)` 据此得出 `FollowAction`：

| 结局 | 探针结论 | 动作 |
|------|----------|------|
| `Cancelled`（切容器 / Clear） | — | `End`：不重连、不提示 |
| `Eof` | `Stopped` / `Gone`（容器已停止或被删除） | `End`：日志流本就该结束 |
| `Eof` | `Running`（容器还在跑却 EOF） | `Recover`：按退避重连（Docker 侧「干净关闭」不应让运行中容器的日志静默停摆） |
| `Eof` | `Unreachable`（Docker 整体不可达） | `Recover`：等它回来 |
| `Idle`（超过 `LOG_IDLE_TIMEOUT = 180s` 无任何日志行） | 连接可能挂死 | `KeepAlive`：**静默**重连（`IDLE_RETRY_DELAY = 1s`），不打扰用户、**不计入失败预算**（长连接健康检查，不是失败） |
| `Error` + 致命（`LogError.status_code == 404` / `No such container`） | 容器已被删除 | `End`：避免僵尸任务无限重试 |
| `Error` + 可恢复 | — | `Recover`：按 `retry_delay` 退避 1s → 2s → 4s → 8s → 10s（上限） |

连续失败预算 `MAX_RETRY_ATTEMPTS = 8` 耗尽后转 30s 低频保活（`KEEPALIVE_INTERVAL`），**永不放弃跟随**——旧实现会静默停止，用户看到日志停在半截却毫无提示。退避等待可被取消；重连沿用同一 stream epoch，期间不新建流。重连时若本流已推送过日志则改用 `tail=0`（只接续新行），否则沿用原 tail，避免重放历史造成重复行。

**UI 反馈**：日志流状态走**独立通道** `Msg::LogStreamStatus(epoch, text)`，写入日志面板工具栏的状态位（`ui/app.slint` 的 `log_status` → `Drawer.log_status`），不占用底部状态栏，也不受操作 toast 的 2s「就绪」定时器影响；带 epoch 过滤，旧流的提示不会落到新容器。文案唯一定义处是 `src/log_status.rs` 的 4 条 i18n key 常量（`NOTICE_INTERRUPTED` / `NOTICE_RECOVERED` / `NOTICE_STOPPED` / `NOTICE_SAMPLED`，见规范 #14 四态）：

| 提示 | 触发时机 |
|---|---|
| 「日志流中断，正在重连…」 | 进入 `Recover` 且此前未提示过（每条流只提示一次，不刷屏） |
| 「日志流已恢复连接」 | ① 重连后收到**首行日志**；② `Idle` 结局（连接已保持 180s 证明流通）；③ 探针从「不可达」跃迁到「可达」。三者均要求此前已提示过「正在重连」 |
| 「日志流已停止」 | `End` 且非 `Cancelled`（容器停止 / 被删除 / 致命错误） |
| 「日志洪峰：已按比例抽样显示，省略 N 行」 | 通道积压超 `LOG_BACKLOG_HIGH`（2 万行）期间每 2s 刷新一次，退出抽样发空串清除（仅单流；分组合并日志的多流共享 epoch，不发此提示，见规范 #33） |

**状态清除**：切换选中资源或按 Clear 走 `ui_apply::clear_log_panel`（日志行模型 + 状态提示全清）；同一容器重建日志流走 `clear_log_status`（只清状态、保留已缓冲的日志行）——新流没有「正在重连」之类的历史状态，但旧内容仍有效。`AppState::ensure_follow` 返回「是否新建了流」，调用方据此决定是否清残留提示。

**流存活标志**：每条流持有独立的 `Arc<AtomicBool>`（`AppState.log.follow_active`）。`ensure_follow` 依据「id 相同 **且** 流仍存活」才幂等返回；旧流退出不会误标新流。这样 `LogStreamEnded` 已入队但尚未 drain 的窗口内切 Tab 也能正确重建流（不再出现「本次切 Tab 不建流、需再切一次」）。跨流共享的 running 标志已移除——容器存活一律以探针结论为准。

### 日志行模型：只追加，不重建（`src/ui_state/log_sync.rs`）

**结论：日志行模型是日志窗口的镜像，用「全局行流位置」对齐；能追加就追加，只有窗口裁剪过前段或换流时才整表替换。**

日志面板用**虚拟化 `ListView`**（逐行 `LogLineRow` delegate），只实例化视口内的行。但虚拟化只解决「渲染多少行」，不解决「模型怎么变」——每批日志都整体替换模型时，可见行会被标脏重排，卡顿依旧（实测 500 行缓冲时软件渲染器吃掉一个 CPU 核）。

| 位置 | 含义 |
|------|------|
| 窗口 = `[logs_total - logs.len(), logs_total)` | `AppState.log.logs`（滚动窗口，≤ `LOG_ROWS_MAX = 2000` 行） |
| 模型 = `[model_total - model_rows, model_total)` | UI 侧 `VecModel<LogRow>` |

- **起点相同** → 模型是窗口的前缀 → `LogSyncPlan::Append { from }`：只 `extend` 新行（1 个 `RowAdded` 事件），已有行索引不变，可见行不重排 → **不卡**。
- **起点不同** → 窗口裁剪过前段（或换了流）→ `LogSyncPlan::Reset`：`set_vec` 整体替换（1 个 reset 事件）。分块裁剪（`LOG_ROWS_KEEP = 1000`）保证这一步每 1000 行才发生一次。

裁剪必须**分块**而不是逐行丢最旧行：逐行 `drain(0..1)` 会让整表索引每行错位一次，Slint 的 ListView 每次都把可见行标脏重排——这正是旧实现卡顿的主因。

> [!NOTE]
> 断流补齐：`json-file` 驱动的跟随流一律带 `timestamps` 建立游标；断裂后的重连带 `since=游标秒` 并把断流窗口内的行补回（纳秒游标过滤重叠段、单次回放只保留最新 `REPLAY_MAX = 2000` 行）。首次建流与 `Clear` 永不带 `since`（避免历史复活 / 清空失效）；非 `json-file` 驱动退回 `tail=0` 旧行为。

## 3. Rust 回调绑定三段式

**结论：回调只做三件事——锁内取 handle/tx、锁外 spawn、结果回 `tx.send(Msg)`；任何耗时操作都不许在锁内发生。**

```text
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

> [!WARNING]
> 锁内**绝不**做任何可能耗时的操作（包括 await、invoke_from_event_loop）。锁内 await 会占着 `AppState` 等 Docker 返回，整个 UI 线程随之卡死；锁内 invoke 则直接触发重入死锁。

## 4. Rust 拆分原则

**结论：按「谁独占什么资源」拆文件，而不是按功能大小拆；每个模块只有一个写入方或一个消费者。**

| 模块 | 职责边界 |
|------|----------|
| `main.rs` | 纯组装：i18n → AppState → MainWindow → 各模块启动 → `ui.run()` → 优雅退出，不写业务 |
| `consumer.rs` | 独占 Receiver + 分批 drain/节流 apply 两阶段泵；列表刷新由独立的 `event_watch.rs` 发起，6s stats、30s 磁盘占用与首屏加载在 `bootstrap.rs` |
| `detail_fmt/` | 详情字段格式化；`ui_state.rs` 只管状态变更与后台任务 |
| `ui_update.rs` | 只做 Msg 聚合：`DrainResult` + 有上限 `drain_messages` + 单条 `apply_msg`，保持日志洪峰防护逻辑独立 |
| `ui_state.rs` | 只做状态与后台任务：刷新去重/超时、follow 建流、清理计数、配置持久化 |
| `ui_state/log_sync.rs` | 日志窗口裁剪与行模型同步账：`LogSyncPlan` / `trim_logs` / `log_sync_plan` / `commit_log_sync` 与其单测独立成模块，`ui_state.rs` 保持 ≤300 行 |
| `callbacks*.rs` | 按实体分：`callbacks.rs`（顶栏+弹窗+配置恢复）+ `callbacks_container.rs` + `callbacks_image.rs` |
| `slint_gen.rs` | 类型集中入口：只做 `include_modules!()` 一次展开 |
| 线程纪律 | **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**，主线程不持有独立 tokio runtime；**所有 Docker 操作结果都走 `tx.send(Msg)`**，不直接碰 Slint Model，统一由 consumer pump 更新 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [架构总纲](../architecture.md) | [README 文档导航](../../README.md#文档地图) | [UI 层架构](ui-interaction.md) |

## 5. 空闲基线怎么测

改任何「常驻开销」相关代码前后，用同一个脚本拿到可比数字：

```bash
python3 scripts/bench-idle.py 240     # 每 10s 采样 RSS / 线程数，末尾给 MB/min 斜率
```

口径与当前基线（v0.7.17，23 容器，窗口 1280×720 可见）：

| 指标 | 实测 |
|------|------|
| 空闲 CPU（1s 分辨率平均） | 0.20% – 0.33% |
| RSS | 启动 48–51MB → 约 100s 后稳定在 54MB（**不是泄漏**，是有界热身后趋平） |
| 线程数 | 33–35（tokio 阻塞池按需伸缩，不单调增长） |
| 6s stats 整轮 | `one_shot` 前 2.03s，后 2.6–3ms |
| 30s `/system/df` 整轮 | 39–160ms（守护进程侧为主，应用 CPU 0–10ms） |
| 进程 → 窗口创建 | 25–34ms |
| 进程 → 首屏容器行 | 153–172ms（中位 159ms；旧的 `join!` 全到齐再推是 181–209ms） |
| 进程 → 首屏镜像行 | 273–292ms（镜像最慢一路，让容器行先上屏） |
| 四列表并发一轮（`bootstrap join` / 每次刷新） | 143–165ms（`/images/json` 92ms 为最慢一路，`/containers/json` 34ms、网络 8ms、卷 8ms） |
| apply（Rust 侧：行构建 + 模型同步） | 23 行 12µs + 5µs ≈ **17µs/轮**；200 行 86µs + 48µs ≈ 134µs |
| 日志热路径（每行） | 正常段 71ns/行（剥前缀+拼前缀+入批，≈1400 万行/s 单线程）；回放段 23ns/行；抽样判定 0.2ns/行 |

apply 这一段单独量（临时基准，`container_row` ×200 轮取均值 + `render::containers` 的差量同步两条路径：全等走比对、全变走 `set_row_data`）。**结论是数据管线已经到地板**：一次 6s stats 应用在 23 容器下只消耗 17µs，折算 0.0003% CPU。空闲 CPU 那 0.20–0.33% 基本不是它——是 Slint 软件渲染器在每次 apply 后按脏区重绘整窗（23 行的百分比文字 + 微条 + 概览带 + 底栏合计会同时变），要再往下压只能减少重绘面积或降采样频率，那是 UI 行为取舍，不是代码效率问题。同一轮还量了「名称排序比较器零分配化」：`to_lowercase` 4.2ms vs `chars().flat_map(char::to_lowercase)` 6.1ms（200 项 ×200 次排序），更慢，**不要改**。

日志热路径单独量，跑 `cargo test --release bench_log_pipeline -- --ignored --nocapture`（`src/docker/log_batch.rs` 的 `bench` 模块，只覆盖应用侧每行成本，不含 Docker 流读取与 UI 行模型同步）。它只在改 `log_ts` / `log_batch` 热路径时才需要复测——按 76k 行/s 的真实码率折算，71ns/行 ≈ 每 1.3 万行消耗 1ms CPU，不是瓶颈。

**首屏那 77ms 的冷启动开销不要去「预热」**：同一条 `/containers/json` 进程内首次请求 73–99ms、curl 热态 33–35ms，差额是连接建立 + 首个请求的固定成本。试过两种预热都被数据否掉：窗口创建前发 `/_ping`（`DP_PREFETCH` 实验，首帧 157ms vs 159ms，无差异），以及提前构造 `docker::connect()`（实测 0.0ms，bollard 客户端是惰性构造）——前者还因为共享 `Arc<Transport>` 的 keep-alive 串行而抢占同一条连接。**要省这 40ms 只能让首屏请求更早发出，而 `ui.run()` 之前投递的 `invoke_from_event_loop` 会被 winit 丢弃**（见 `AGENTS.md` 的焦点引导坑条），所以这是架构边界，不是可调参数。

测的时候三点必须对齐：release 构建、`WAYLAND_DISPLAY` 可用、无其它实例占单实例锁（否则启动即退出，采样全是 0）。
