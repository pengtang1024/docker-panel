# Docker Panel 架构设计

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
├── ui/
│   ├── app.slint               Slint 主骨架：AppActions global + MainWindow + import 组装其余 4 文件
│   ├── models.slint            ContainerRow / ImageRow struct（Rust ↔ Slint 数据桥）
│   ├── delegates.slint         行 delegate（ContainerRowDelegate / ImageRowDelegate），callback 属性注入
│   ├── panes.slint             TopBar / ToolBar / StatusBar 横向 strip 组件
│   └── dialogs.slint           PullDialog + CleanupDialog 弹窗组件
└── build.rs                    slint_build::compile("ui/app.slint")
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

## 4. Slint UI 模块交互

5 个 `.slint` 文件按职责拆分，依赖单向：**app.slint 是唯一的组装入口**，它 import 其余 4 个文件；子文件之间**不互相 import**（避免循环依赖和跨文件 global 可见性问题）。

### 交互总图

```
                           app.slint（组装层）
                          ╱  ╲   ╱  ╲   ╲
                         ╱    ╲ ╱    ╲   ╲
            models.slint  delegates.slint  panes.slint  dialogs.slint
            (数据桥)        (行视图)         (横向 strip)   (弹窗)
                 │              │              │              │
                 │              │              │              │
                 ▼              ▼              ▼              ▼
          Rust main.rs    Rust callbacks.rs  Rust 回调绑定    Rust 回调绑定
         build_*_rows()   ui.global::<AppActions>()
         ModelRc::from()   actions.on_xxx(...)
```

### 四种交互模式

#### ① models.slint → Rust main.rs：property 数据注入

```
models.slint 定义 struct：          Rust 侧构造 → ModelRc 注入：
┌──────────────────────┐          ┌─────────────────────────┐
│ struct ContainerRow {│          │ let rows = state.lock() │
│   id, name, image,   │          │   .containers.iter()    │
│   status, is_running │          │   .map(build_row)       │
│ }                    │          │   .collect::<Vec<_>>(); │
│ struct ImageRow {    │          │ ui.set_container_rows(  │
│   id, name, tag,     │          │   ModelRc::from(&rows)  │
│   size, containers   │          │ );                      │
│ }                    │          └─────────────────────────┘
└──────────────────────┘
         │
         │ include_modules!() 生成 Rust 类型
         ▼
   main.rs 直接 new ContainerRow { ... }
```

**关键点**：struct 字段不能随便改名——Rust 侧 `build_container_rows` 直接构造它们。`models.slint` 存在的唯一意义就是把数据桥声明抽离出来，让 app.slint 只管布局。

#### ② delegates.slint ← app.slint：callback 属性注入

```
delegates.slint 声明接口：            app.slint 的 ListView 循环里注入实现：
┌─────────────────────────┐        ┌────────────────────────────────────┐
│ component ContainerRow  │        │ for row in root.container_rows:    │
│   Delegate {            │        │   ContainerRowDelegate {           │
│     callback toggle();  │        │     row_id: row.id;               │
│     callback restart(); │        │     toggle => {                    │
│     callback remove();  │        │       AppActions.container_toggle  │
│   }                     │        │         (row.id);                  │
│   Button {              │        │     }                              │
│     clicked => {        │        │     restart => { ... };            │
│       root.toggle();    │        │     remove => { ... };             │
│     }                   │        │   }                                │
│   }                     │        └────────────────────────────────────┘
└─────────────────────────┘
```

**为什么 delegates 不直接 import AppActions global？** 因为 Slint 跨文件 import 的 global 在 Rust 侧 `include_modules!()` 展开后类型不兼容。改成 callback 属性注入后，delegates 完全独立——它只声明"我需要 toggle/remove"，谁来实现是 app.slint 的事。

#### ③ panes.slint ↔ app.slint：in-out 属性双向绑定 `<=>`

```
panes.slint 声明组件：                 app.slint 组装：
┌────────────────────────┐           ┌───────────────────────────────┐
│ component TopBar {     │           │ TopBar {                       │
│   in-out property      │  <=>      │   docker_ok <=> root.docker_ok;│
│     <bool> docker_ok;  │──────────▶│   refresh => {                 │
│   callback refresh();  │  callback │     root.refresh_requested(); │
│   callback search(s);  │           │   }                            │
│ }                      │           │   search(val) => {             │
│                        │           │     root.search_accepted(val);│
│ component ToolBar {    │           │   }                            │
│   callback stop_all(); │           │ }                              │
│   callback pull_...(); │           └───────────────────────────────┘
│   callback cleanup_..();│
│ }
│                        │
│ component StatusBar {  │
│   in-out property      │
│     <string> status_.. │
│     <string> container │
│     <string> image_... │
│ }
└────────────────────────┘
```

**`<=>` 双向绑定**：`StatusBar.status_text <=> root.status_text` 意味着 panes 里任何属性变更都会同步到 MainWindow 的 root 属性，反之亦然。callback 是单向的——panes 触发，MainWindow 实现并转发到 Rust 回调。

#### ④ dialogs.slint ← app.slint：条件渲染 + in-out 双向绑定

```
dialogs.slint 声明组件：               app.slint 条件渲染：
┌─────────────────────────┐          ┌──────────────────────────────────┐
│ component PullDialog {  │          │ if root.pull_open: PullDialog { │
│   in-out property      │          │   input <=> root.pull_input;   │
│     <string> input;    │          │   busy <=> root.pull_busy;     │
│   in-out property      │  <=>     │   close => { root.pull_close(); }│
│     <bool> busy;       │          │   submit(val) => {              │
│   callback close();    │          │     root.pull_submit(val);      │
│   callback submit(s);  │          │   }                            │
│ }                      │          │ }                              │
│                        │          │ if root.cleanup_open:           │
│ component CleanupDialog│          │   CleanupDialog { ... }        │
│   in-out property      │          └──────────────────────────────────┘
│     <bool> stopped;    │
│   in-out property      │
│     <bool> dangling;   │   条件由 Rust 侧 ui.set_pull_open(true/false) 控制
│   in-out property      │
│     <bool> unused;     │
│   callback close();    │
│   callback submit(..); │
│ }
└─────────────────────────┘
```

**条件渲染 + 双向绑定**是标准弹窗模式：`if root.pull_open:` 控制遮罩层显示/隐藏，`<=>` 双向绑定把弹窗内的 LineEdit、CheckBox 值同步回 MainWindow 的 root 属性（Rust 侧通过 `ui.set_pull_input()` 预填）。

#### Rust 侧绑定 AppActions global

```
app.slint:                    Rust callbacks.rs:
export global AppActions {
  callback container_toggle(string);  │  let actions = ui.global::<AppActions>();
  callback container_restart(string); │  actions.on_container_toggle(|id| {
  callback container_remove(string);  │    // spawn stop/start → tx.send(Msg::ActionDone)
  callback image_remove(string);      │  });
}                                     │  actions.on_image_remove(|id| {
                                     │    // spawn remove_image → tx.send(...)
                                     │  });
```

`AppActions` 是 app.slint 里**唯一的 global**，只在 app.slint 声明、只在 callbacks.rs 绑定。delegates.slint 通过 callback 属性间接调用它，而不是直接 import——这是跨文件安全绑定的关键设计。

### 文件清单速查

| 文件 | 行数 | 职责 | import 谁 | 被谁 import |
|------|------|------|-----------|-------------|
| `app.slint` | ~148 | MainWindow 骨架 + AppActions global | models / delegates / panes / dialogs | （build.rs 入口文件） |
| `models.slint` | ~21 | ContainerRow / ImageRow struct | 无 | app.slint |
| `delegates.slint` | ~136 | ListView 每行视图 + callback 属性 | 无 | app.slint |
| `panes.slint` | ~80 | TopBar / ToolBar / StatusBar | 无 | app.slint |
| `dialogs.slint` | ~157 | PullDialog / CleanupDialog | 无 | app.slint |

### 数据桥 struct（来自 models.slint）

```rust
// Rust 侧 slint_gen.rs 的 include_modules!() 自动生成：
// crate::slint_gen::ContainerRow { id, short_id, name, image, status, is_running }
// crate::slint_gen::ImageRow { id, short_id, name, tag, size, containers }
```

### AppActions global（来自 app.slint）

```rust
// crate::slint_gen::AppActions {
//   on_container_toggle(impl FnMut(SharedString)),
//   on_container_restart(impl FnMut(SharedString)),
//   on_container_remove(impl FnMut(SharedString)),
//   on_image_remove(impl FnMut(SharedString)),
// }
```

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

单文件超过 300 行必须按职责拆分。当前所有文件均 ≤ 300 行：main.rs ~243、callbacks.rs ~299、ui_state.rs ~249、delegates.slint ~136、dialogs.slint ~157、panes.slint ~80。

拆分原则（Slint 侧）：
- **app.slint 做组装层**——只放 MainWindow 骨架 + AppActions global + import 语句，**不**堆内联 markup
- **子文件之间不互相 import**——避免循环依赖和跨文件 global 可见性问题
- **delegates 用 callback 属性注入**代替直接 import AppActions global，绕开 Rust 侧 glob import 兼容性坑
- **panes/dialogs 用 in-out 属性 `<=>` 双向绑定**——数据流向一眼可见

拆分原则（Rust 侧）：
- **桥接层分离**：main.rs（consumer 线程 + pump + 入口）与 callbacks.rs（UI 回调绑定到 AppActions）分离
- **类型集中入口**：`slint_gen.rs` 只做 `include_modules!()` 一次展开，其余 Rust 模块通过 `use crate::slint_gen::*` 导入，避免多文件独立展开导致类型不兼容
