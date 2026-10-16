# dvkit 架构设计

v0.2.0 完全重写版：Slint 原生 GUI + 纯软件渲染器。本文档随仓库版本管理，描述当前实际架构。

## 1. 分层与模块职责

```
main.rs                         桥接层：consumer 线程 + 定时刷新 + 回调绑定
├── ui_state.rs                 AppState（可共享状态）+ drain_messages + derive_update + format_bytes
├── models.rs                   纯数据结构：ContainerInfo / ImageInfo / Msg / ResourceSample
├── docker/
│   ├── mod.rs                  connect() + pub use 重导出
│   ├── containers.rs           列表 / 启停重启删除 / 采样
│   ├── images.rs               列表 / 拉取（含进度回调闭包）/ 删除
│   └── system.rs               cleanup prune / follow_logs（v0.2 未暴露日志窗口）
├── app.slint                   Slint UI 声明：MainWindow + ContainerRowDelegate + ImageRowDelegate + Pull/Cleanup 弹窗
└── build.rs                    slint_build::compile("app.slint")
```

依赖单向：`main → ui_state / models / docker`；`ui_state` 依赖 `models` 和 `docker` 的列表函数（用于 refresh）；`docker` 是纯 async 层，不反依赖任何上层模块。

## 2. 线程模型（三线程架构）

```
主线程 (ui.run() 阻塞)            consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ Slint Timer          │       │ 独占 Receiver<Msg>     │       │ spawn 异步任务      │
│   3s → refresh()     │       │                        │       │ bollard API 调用    │
│ UI 回调              │  lock  │ 1. pump（扫积压）       │       │ tx.send(Msg)        │
│   → spawn 操作       │──────▶│ 2. recv() 阻塞（锁外） │◀──────│                     │
│                      │       │ 3. 收到 → lock state    │       │                     │
│                      │       │    drain_messages       │       │                     │
│                      │       │    build rows           │       │                     │
│                      │       │    drop lock            │       │                     │
│                      │       │ 4. invoke_from_event   │       │                     │
│                      │       │    _loop 更新 Slint     │       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
       Mutex<AppState>                 mpsc::channel()           spawn(async { ... })
```

### 关键设计决策

**Receiver 不进 Mutex**

`mpsc::Receiver<Msg>` 由 consumer 线程独占所有权，**不**放在 `AppState` 里。这样 `rx.recv()` 永远不会阻塞主线程拿 state 锁——彻底避免死锁的唯一保证。

如果把 Receiver 放进 Mutex，consumer 在 `recv()` 阻塞时持有锁，主线程任何想 `state.lock()` 的操作（Timer 回调、UI 回调）都会卡死，窗口完全失去响应。

**锁持有时间最小化**

consumer 的 pump 流程：

```rust
let Ok(mut s) = state.lock() else { return };
let (need_update, latest_msg) = s.drain_messages(rx);
let container_rows = build_container_rows(&s);  // 纯读取
let image_rows = build_image_rows(&s);          // 纯读取
// ... 取几个字段 ...
drop(s);   // ⚠️ 必须在 invoke_from_event_loop 之前释放

invoke_from_event_loop(move || {
    // 闭包里不再碰 state
    ui.set_container_rows(ModelRc::from(container_rows.as_slice()));
    // ...
}).ok();
```

锁持有 = drain + build rows，微秒级。`invoke_from_event_loop` 从主线程事件队列取，闭包里如果再 lock state 会产生经典的"主线程等 consumer 释放锁，consumer 等主线程事件队列消费完"互锁。

**主线程 Timer 与 Consumer 的协作**

主线程 Slint Timer 3s 触发一次 `refresh()` → spawn 两个列表任务 → `tx.send(Msg::Containers)` + `tx.send(Msg::Images)`。consumer 线程正在 `recv()` 阻塞，被新消息唤醒后 pump → 更新 Slint Model。

Timer 回调本身**不**直接碰 Slint Model——它只负责"发任务"，consumer 统一接收后调用 `invoke_from_event_loop` 更新 UI。这符合 Slint 线程安全模型：只有主线程可以修改 Model。

## 3. 消息流

```
                    tokio spawn                           mpsc channel
┌──────────┐  refresh()  ┌──────────────┐  tx.send()   ┌───────────┐
│ 主线程    │────────────▶│ 后台 async    │────────────▶│  Receiver  │
│ Timer    │  /  UI 操作  │ bollard API  │              │ (consumer) │
└──────────┘             └──────────────┘              └─────┬─────┘
                                                             │
                          ┌──────────────────────────────────┘
                          ▼
                   ┌──────────────┐      invoke_from_event_loop
                   │  lock state   │      ──────────────────────▶ 主线程
                   │  drain        │                             ┌──────────────┐
                   │  build rows   │                             │ ui.set_*()   │
                   │  drop lock    │                             │ ModelRc::from│
                   └──────────────┘                             └──────────────┘
```

### Msg 类型一览

| 变体 | 触发 | UI 影响 |
|------|------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | 定时刷新 / 手动刷新 | 替换容器列表，`docker_ok=true`/状态文本=错误 |
| `Images(Result<Vec<ImageInfo>>)` | 定时刷新 / 手动刷新 | 替换镜像列表 |
| `Sample(id, Result<ResourceSample>)` | — | v0.2 未使用（资源监控已移除） |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 成功→关弹窗+刷新镜像；失败→状态文本+reset pull_busy |
| `ActionDone(desc, Result<String>)` | 启停重启删除 | 状态文本 + consumer 随后会收到 Containers/Images 刷新（由操作函数触发？不——当前实现是操作只发 ActionDone，不主动刷新列表；下次 Timer 3s 会刷新） |
| `CleanupDone(Result<String>)` | prune 完成 | 状态文本 + consumer 随后收到列表刷新 |

> 注意：当前版本操作完成后**不立即刷新列表**，等下一个 3s Timer 周期。这是可接受的简化，后续如果操作后需要立即可见，可以在 ActionDone/CleanupDone 分支里同步 spawn 一次列表刷新。

## 4. Slint UI 声明

### 数据绑定

```slint
export struct ContainerRow { id, short_id, name, image, status, is_running }
export struct ImageRow { id, short_id, name, tag, size, containers }
```

Rust 侧通过 `slint::include_modules!()` 生成 `ContainerRow` / `ImageRow` / `MainWindow` 的 Rust 类型。更新列表时用 `ModelRc::from(container_rows.as_slice())` 创建只读 Model 注入。

### 全局 Action 回调

```slint
export global AppActions {
    callback container_toggle(string);
    callback container_restart(string);
    callback container_remove(string);
    callback image_remove(string);
}
```

delegate 组件内通过 `AppActions.container_toggle(root.row_id)` 委托给 Rust 侧。Rust 侧 `actions.on_container_toggle(...)` 绑定 handler。

### 弹窗状态

Pull 和 Cleanup 弹窗都是 `if root.pull_open:` / `if root.cleanup_open:` 条件渲染的全屏遮罩。关闭触发：点遮罩区域或点取消按钮。

## 5. Docker 接入约定

- 连接：`Docker::connect_with_local_defaults()`，自动读 `DOCKER_HOST`，默认 `/var/run/docker.sock`
- 错误统一转中文 `String` 上抛，UI 层展示
- 删除容器 `force=true`，运行中也能删
- 删除镜像 `force=true`，被引用时 Docker 会拒绝，错误直接展示
- 清理逻辑：
  - `stopped` → `prune_containers()`
  - `dangling` → `prune_images(dangling=true)`
  - `unused` → `prune_images(dangling=false)`（等价 `docker image prune -a`）
  - 三个选项**可以任意组合**同时执行

## 6. 文件规范

单文件超过 300 行必须按职责拆分。当前 `main.rs` 约 440 行（含回调绑定），下一步应拆出 `callbacks.rs` 把 `bind_callbacks` 独立出去。

> 历史：v0.1 的 egui 架构有 `app/`、`widgets/`、`utils/` 三个模块目录，v0.2 重写时全部删除，功能收敛到最少必要文件。
