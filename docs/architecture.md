# Docker Panel 架构设计

v0.2.0 重写版：Slint 原生 GUI + 纯软件渲染器 + 三线程消息架构。
本文档描述当前实际代码状态，随仓库版本管理。

写代码时必须按下方精确版本的 API 写，禁止凭旧版本记忆。
更完整的版本特有 API 限制清单见仓库内 `AGENTS.md`。

## 0. 技术栈（精确版本）

| 组件 | 精确版本 | Feature / 备注 |
|------|----------|----------------|
| Rust | stable | edition 2021 |
| Slint | **1.14** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-2` + `std`；TabWidget 无 `current-tab`、delegate 无 MouseArea、子组件不继承 parent 尺寸——详见 AGENTS.md「Slint 1.14 版本特有限制」 |
| slint-build | **1.14** | build-dependency；`slint_build::compile("ui/app.slint")` |
| bollard | **0.21** | Docker Engine API 纯 async 客户端；`LogOutput` 是 struct 不是 tuple——详见下方「bollard 0.21 易踩坑速查」 |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false）；必须多线程 runtime 不能 current_thread；spawn 前必须 `handle().clone()`——详见 AGENTS.md「tokio 1.x 版本关键用法」 |
| futures | **0.3** | — |
| futures-util | **0.3** | 提供 `StreamExt`（`.next().await`），需显式引入 |
| 渲染 | — | 纯软件渲染器（`renderer-software`），**无 Vulkan/GL/WGPU 依赖**，deb 包不依赖 `libgl1` |
| 构建优化 | — | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip（release profile） |

## 1. 模块地图与文件交互

```
docker-panel/
├── Cargo.toml                依赖：slint 1.14 / bollard 0.21 / tokio / futures
├── build.rs                  slint_build::compile("ui/app.slint")
│
├── src/                       Rust 层（所有 ≤ 300 行）
│   ├── main.rs                桥接中枢：consumer 线程 + 定时刷新 + pump_and_apply
│   ├── slint_gen.rs           include_modules!() 集中入口（仅 7 行）
│   │
│   ├── models.rs              纯数据结构：Msg / ContainerInfo / ImageInfo / ContainerDetail
│   │
│   ├── ui_state.rs            AppState（Arc<Mutex<>> 可共享状态）
│   │                          + drain_messages / handle_one / build_detail_data
│   │
│   ├── ui_update.rs           derive_update(Msg → UiUpdate) + UiUpdate 结构体
│   │                          （从 ui_state.rs 拆出，86 行）
│   │
│   ├── ui_apply.rs            apply_detail(&ui, &DetailTuple) — 把 17 字段灌入 DetailPanel
│   │                          apply_lists(&ui, cm, im, lm) — 三个 ModelRc 一次性 set
│   │                          （从 main.rs 的 invoke_from_event_loop 闭包拆出，42 行）
│   │
│   ├── callbacks.rs           顶栏按钮 + PullDialog + CleanupDialog 回调绑定
│   ├── callbacks_container.rs 容器启停/重启/删除/logs/inspect（AppActions global）
│   ├── callbacks_image.rs     镜像删除（AppActions global）
│   │
│   └── docker/
│       ├── mod.rs             connect() + pub use 重导出
│       ├── containers.rs      list / start / stop / restart / remove
│       ├── images.rs          list / pull（含进度闭包）/ remove
│       ├── logs.rs            inspect_container / follow_logs（Stream → tx.send）
│       └── system.rs          cleanup prune
│
└── ui/                        Slint 层（所有 ≤ 300 行）
    ├── app.slint              主骨架：MainWindow + AppActions global + import 组装 + 布局
    ├── models.slint           ContainerRow / ImageRow / LogLineRow struct（Rust↔Slint 数据桥）
    ├── delegates.slint        行 delegate（ContainerRowDelegate / ImageRowDelegate）callback 属性注入
    ├── headers.slint          ContainerHeader / ImageHeader（列宽严格对齐 delegates）
    ├── panes.slint            TopBar / ToolBar / StatusBar 三个横向 strip
    ├── dialogs.slint          PullDialog + CleanupDialog 弹窗
    ├── detail-panel.slint     DetailRow + DetailPanel（17 字段 in-out 属性）
    └── log-panel.slint        LogLineDelegate + LogPanel（ListView 滚动）
```

### 依赖方向（严格单向，无循环）

```
main.rs
  ├── ui_state.rs    → models.rs + docker::{list_containers, list_images, inspect_container}
  ├── ui_update.rs   → models.rs
  ├── ui_apply.rs    → slint_gen (MainWindow) + ui_state::DetailTuple
  ├── callbacks.rs   → slint_gen + ui_state::SharedState + docker
  ├── callbacks_container.rs / callbacks_image.rs → 同 callbacks.rs
  └── docker/        → 纯 async 层，不反依赖任何上层模块

ui/*.slint           app.slint 是唯一组装入口，import 其余 7 个子文件，
                     子文件之间不互相 import（避免 Slint 跨文件 global 兼容性坑）
```

## 2. 线程模型（三线程架构）

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ Slint Timer 3s ──lock─▶│ recv() 阻塞（锁外）           │       │ spawn 异步任务      │
│ 手动刷新 ───────lock─▶│ drain_messages            │       │ bollard API 调用    │
│ UI 回调 ────────lock─▶│ build_container_rows      │       │ tx.send(Msg)        │
│                        │ build_image_rows          │◀──────│ follow_logs 流      │
│                        │ build_detail_data          │       │ inspect_container   │
│                        │ build_log_lines            │       │                     │
│                        │ drop(s) ──────────────────lock release
│                        │ invoke_from_event_loop     │       │                     │
│                        │   set_d_xxx / set_log_lines│       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
                                     （consumer 独占，不进 Mutex）
```

### Receiver 不进 Mutex — 死锁唯一保证

`mpsc::Receiver<Msg>` 由 consumer 线程独占所有权，**不**放在 `AppState` 里。
这样 `rx.recv()` 永远不会阻塞主线程拿 state 锁。
如果 Receiver 放进 Mutex，consumer 阻塞 `recv()` 时持有锁 → 主线程任何 `state.lock()`（Timer 回调、UI 回调）全卡死。

### pump_and_apply 两阶段铁律

```rust
// Phase 1 — 锁内（微秒级）：
let Ok(mut s) = state.lock() else { return };
let (need_update, latest_msg) = s.drain_messages(rx, first);
let cm_rows = build_container_rows(&s);    // 纯读取
let im_rows = build_image_rows(&s);        // 纯读取
let detail = s.build_detail_data();         // 17 字段 SharedString 元组
let log_lines: Vec<LogLineRow> = s.logs.iter().map(...).collect();
let running_total = format!("{}/{}", running, s.containers.len());
let update = derive_update(len, &latest_msg);
drop(s);  // ⚠️ 必须在 invoke_from_event_loop 之前释放

// Phase 2 — 锁外：
invoke_from_event_loop(move || {
    let Some(ui) = ui_w.upgrade() else { return };
    ui_apply::apply_lists(&ui, cm, im, lm);
    ui_apply::apply_detail(&ui, &detail);  // None 时无操作
    ui.set_container_count(SharedString::from(running_total));
    // ... status_text / docker_ok / pull_open / pull_busy ...
}).ok();
```

### 主线程 Timer 与 Consumer 的协作

Slint Timer 3s → `refresh()` → spawn 两个列表任务 → `tx.send(Msg::Containers)` + `tx.send(Msg::Images)`。
consumer 正在 `recv()` 阻塞，被新消息唤醒后 pump → 更新 Slint Model。
Timer 回调本身**不**碰 Slint Model——只负责发任务，consumer 统一调用 `invoke_from_event_loop` 更新 UI。
这符合 Slint 线程安全模型：只有主线程可以修改 Model。

## 3. 消息流（Msg 变体一览）

| 变体 | 触发源 | UI 行为 |
|------|--------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | Timer 刷新 / 手动刷新 | 替换容器列表 + 剪失效选择 |
| `Images(Result<Vec<ImageInfo>>)` | Timer 刷新 / 手动刷新 | 替换镜像列表 + 剪失效选择 |
| `PullProgress(String)` | `pull_image` 进度回调闭包 | 状态文本更新 |
| `PullDone(Result<String>)` | pull_image 结束 | 关弹窗 + 刷新镜像列表 |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本（下次 Timer 会自动刷新列表） |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 下次 Timer 刷新 |
| **`ContainerDetail(Result<ContainerDetail>)`** | `inspect_container` → 详情面板 | 替换 `AppState.detail`，pump_and_apply 灌入 DetailPanel |
| **`LogLine(String)`** | `follow_logs` Stream | 推入 `AppState.logs`（环形缓冲 cap=500），pump_and_apply 灌入 LogPanel |

### 消息处理流水线

```
                     tokio spawn
  ┌──────────┐    refresh()     ┌──────────────┐    tx.send()    ┌───────────┐
  │ 主线程    │────────────────▶│ 后台 async    │──────────────▶│ Receiver  │
  │ Timer    │  /UI 操作/inspect│ bollard API   │                 │ (consumer)│
  │          │                  │ follow_logs   │◀──Stream──────│           │
  └──────────┘                  └──────────────┘                 └─────┬─────┘
                                                                       │
                   ┌──────────────────────────────────────────────────┘
                   ▼
         ┌────────────────┐
         │ lock state      │
         │ drain_messages  │  逐条 handle_one → 更新 containers / images / detail / logs
         │ build rows      │  （调用 build_container_rows / build_image_rows / build_detail_data）
         │ drop lock       │
         └────────┬───────┘
                  │ invoke_from_event_loop
                  ▼
         ┌──────────────────────────────────────────────────┐
         │ 主线程（闭包在 Slint 事件队列里执行）                 │
         │                                                    │
         │  ui_apply::apply_lists(&ui, cm, im, lm)           │
         │    └ ui.set_container_rows(ModelRc::from(...))     │
         │    └ ui.set_image_rows(ModelRc::from(...))        │
         │    └ ui.set_log_lines(ModelRc::from(...))         │
         │                                                    │
         │  if let Some(ref d) = detail_data:                 │
         │    ui_apply::apply_detail(&ui, d)                  │
         │      └ ui.set_d_name / set_d_state / set_d_... ×17│
         │                                                    │
         │  ui.set_container_count / set_docker_ok / ...     │
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

环形缓冲在 `handle_one` 里做（`drain(0..excess)`），避免 `push` 无限增长。

## 4. Slint UI 模块交互

8 个 `.slint` 文件——app.slint 是唯一组装入口，子文件间**不互相 import**。

### 交互总图

```
                               app.slint（组装层 + 布局）
                          ╱ ╲   ╱ ╲   ╱ ╲   ╱ ╲   ╱ ╲
                         ╱   ╲ ╱   ╲ ╱   ╲ ╱   ╲ ╱   ╲
            models.slint  delegates.slint  headers.slint  panes.slint  dialogs.slint
            (数据桥)       (行视图+cb注入)  (表头列宽对齐)   (横向strip)   (弹窗)
                 │              │              │
                 │              │              │
                 └──────┬───────┴──────────────┘
                        ▼
              ListView [容器 Tab] / ListView [镜像 Tab]
                        │
                        ▼
              右侧 TabWidget
           ┌──────────┴──────────┐
           ▼                     ▼
     detail-panel.slint    log-panel.slint
     (17 字段 in-out)       (ListView 滚动)
```

### 四种交互模式

#### ① models.slint → Rust：property 数据注入（不变）

```
models.slint struct           Rust pump_and_apply 构造
┌──────────────────────────┐  ┌─────────────────────────┐
│ ContainerRow { id, ... } │  │ let rows = state.lock() │
│ ImageRow    { id, ... } │  │   .containers.iter()    │
│ LogLineRow  { text }    │  │   .map(build_row)       │
│ ContainerDetail 无 Slint│  │   .collect::<Vec<_>>(); │
│   struct —— Rust 单独定义│  │ ui_apply::apply_lists() │
└──────────────────────────┘  └─────────────────────────┘
```

`include_modules!()` 在 `slint_gen.rs` 一次展开，其余 Rust 模块通过 `use crate::slint_gen::*` 导入。

#### ② delegates.slint ← app.slint：callback 属性注入（不变）

delegates 不 import AppActions global——它只声明 `callback toggle()` / `callback restart()` / `callback logs()` / `callback remove()`。
app.slint 的 ListView 循环里注入实现：

```
ContainerRowDelegate {
    toggle => { AppActions.container_toggle(row.id); },
    restart => { AppActions.container_restart(row.id); },
    logs    => { AppActions.container_logs(row.id); },     // 🆕 Phase 2
    remove  => { AppActions.container_remove(row.id); },
}
```

`AppActions` 只在 app.slint 声明、只在 callbacks_container.rs / callbacks_image.rs 绑定。

#### ③ panes.slint ↔ app.slint：`<=>` 双向绑定（不变）

`StatusBar.status_text <=> root.status_text` / `TopBar.docker_ok <=> root.docker_ok`

#### ④ 详情/日志面板：`<=>` in-out 双向绑定 + TabWidget

```
detail-panel.slint 声明：        app.slint 组装：
┌────────────────────────┐     ┌─────────────────────────────┐
│ DetailPanel inherits   │     │ TabWidget {                 │
│   Rectangle (背景+尺寸) │     │   Tab { title: "详情";      │
│ {                      │     │     DetailPanel {           │
│   in-out property      │  ╱═ │       width: 320px;         │
│     <string> d_name;   │ ╱   │       height: 600px;        │
│   in-out property      │     │       d_name <=> root.d_name│
│     <string> d_state;  │     │       d_state <=> root.d_...│
│   ... ×17              │     │       d_workdir <=> ...     │
│ }                      │     │     }                       │
│                        │     │   }                         │
│ LogPanel {             │     │   Tab { title: "日志";      │
│   in-out property      │     │     LogPanel {              │
│     <[LogLineRow]>     │     │       log_lines <=>         │
│       log_lines;       │     │         root.log_lines;    │
│ }                      │     │     }                       │
└────────────────────────┘     └─────────────────────────────┘
```

**TabWidget 内容需要显式 width/height**——Slint Tab 容器不给子组件自动传 parent 尺寸，必须在 app.slint 的实例化处硬设。

### Slint 文件清单速查

| 文件 | 行数 | 职责 | import 谁 |
|------|------|------|-----------|
| `app.slint` | ~218 | MainWindow + AppActions global + 布局（FixedLayout 绝对定位左右分栏） | models / delegates / headers / panes / dialogs / detail-panel / log-panel |
| `models.slint` | ~24 | ContainerRow / ImageRow / LogLineRow struct | 无 |
| `delegates.slint` | ~121 | 行视图 + callback 属性注入（含日志按钮） | 无 |
| `headers.slint` | ~51 | ContainerHeader / ImageHeader（列宽严格对齐 delegates） | 无 |
| `panes.slint` | ~80 | TopBar / ToolBar / StatusBar | 无 |
| `dialogs.slint` | ~157 | PullDialog / CleanupDialog | 无 |
| `detail-panel.slint` | ~116 | DetailRow + DetailPanel（17 字段） | 无 |
| `log-panel.slint` | ~57 | LogLineDelegate + LogPanel（ListView） | models.slint (LogLineRow) |

## 5. Rust 回调绑定三段式

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

## 6. Docker 接入约定

- 连接：`Docker::connect_with_local_defaults()`，自动读 `DOCKER_HOST`，默认 `/var/run/docker.sock`
- 错误统一转中文 `String` 上抛，UI 层展示
- 删除容器 `force=true`；删除镜像 `force=true`，被引用时 Docker 拒绝
- `inspect_container`：`docker::inspect_container(&id).await → ContainerDetail`
- `follow_logs`：`.follow(true).tail("200")` 返回 Stream，逐行 `tx.send(Msg::LogLine)`
- 清理：`stopped→prune_containers` / `dangling→prune_images(dangling=true)` / `unused→prune_images(-a)`，可任意组合

### bollard 0.21 易踩坑速查

| 项目 | 实际类型 | 备注 |
|------|----------|------|
| `LogOutput` | `LogOutput::StdOut { message: Bytes }` | struct 变体，**不是** tuple |
| `ContainerStateStatusEnum` | 没有 Default | 用 `.as_ref().map(\|e\| e.to_string()).unwrap_or_default()` |
| `NetworkSettings.ports` | `Option<HashMap<String, Option<Vec<PortBinding>>>>` | 双层 Option |
| `PortBinding.host_port` | `Option<String>` | 不是 u16 |
| `LogsOptionsBuilder.tail` | 收 `&str` | 不是 `String` |
| 依赖 | `futures-util = "0.3"` | 提供 `StreamExt` 调 `.next().await` |

## 7. 文件规范

所有源码文件 ≤ 300 行（当前最大 main.rs 269 / ui_state.rs 264）。

**Slint 拆分原则：**
- app.slint 做组装层——只放 MainWindow 骨架 + AppActions global + import 语句 + 布局 markup
- 子文件之间不互相 import——避免循环依赖和跨文件 global 可见性问题
- delegates 用 callback 属性注入代替直接 import AppActions global，绕开 Rust 侧 glob import 兼容性坑
- panes/dialogs 用 in-out 属性 `<=>` 双向绑定——数据流向一眼可见
- **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸

**Rust 拆分原则：**
- 桥接层分离：main.rs（consumer 线程 + pump + 入口）与 callbacks.rs（UI 回调绑定）分离
- **pump_and_apply 闭包内的 set_d_xxx 抽成 ui_apply::apply_detail**——避免 main.rs 被 17 个 set_ 调用撑爆
- **derive_update match 抽成独立 ui_update.rs**——ui_state.rs 只管状态变更，update 推导单独一个文件
- 回调按实体分：callbacks.rs（顶栏+弹窗）+ callbacks_container.rs + callbacks_image.rs
- 类型集中入口：`slint_gen.rs` 只做 `include_modules!()` 一次展开
- **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**——主线程不持有独立 tokio runtime
- **所有 Docker 操作结果都走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新
