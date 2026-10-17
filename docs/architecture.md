# Docker Panel 架构设计

v0.2.0 重写版：Slint 原生 GUI + 纯软件渲染器 + 三线程消息架构。
本文档描述当前实际代码状态，随仓库版本管理。

写代码时必须按下方精确版本的 API 写，禁止凭旧版本记忆。
更完整的版本特有 API 限制清单见仓库内 `AGENTS.md`。

## 0. 技术栈（精确版本）

| 组件 | 精确版本 | Feature / 备注 |
|------|----------|----------------|
| Rust | stable | edition 2021 |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std`；strict 模式已验证编译通过——详见 AGENTS.md「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency；`compile_with_config` + `with_bundled_translations("translations")` |
| bollard | **0.21** | Docker Engine API 纯 async 客户端；`LogOutput` 是 struct 不是 tuple——详见下方「bollard 0.21 易踩坑速查」 |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false）；必须多线程 runtime 不能 current_thread；spawn 前必须 `handle().clone()`——详见 AGENTS.md「tokio 1.x 版本关键用法」 |
| futures | **0.3** | — |
| futures-util | **0.3** | 提供 `StreamExt`（`.next().await`），需显式引入 |
| 渲染 | — | 纯软件渲染器（`renderer-software`），**无 Vulkan/GL/WGPU 依赖**，deb 包不依赖 `libgl1` |
| 构建优化 | — | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip（release profile） |

## 1. 模块地图与文件交互

```
docker-panel/
├── Cargo.toml                依赖：slint 1.18.1 / bollard 0.21 / tokio / futures
├── build.rs                  slint_build::compile_with_config + with_bundled_translations("translations")
│
├── src/                       Rust 层（所有 ≤ 300 行）
│   ├── main.rs                组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs           include_modules!() 集中入口
│   ├── i18n.rs                locale 检测（zh*→zh_CN）+ Rust 侧 t() + slint_language()
│   ├── config.rs              AppConfig 持久化（directories + 手写 JSON 原子写）
│   │
│   ├── consumer.rs            consumer 线程：recv_timeout(200ms) Msg 泵 pump_and_apply + 3s 刷新 interval
│   ├── bootstrap.rs           6s stats 采样 interval + 首屏四列表加载 + inspect 首容器
│   │
│   ├── models.rs              纯数据结构：Msg / ContainerInfo / ImageInfo / ContainerDetail / NetworkInfo / VolumeInfo
│   ├── batch.rs               BatchState 多选状态（purge_dead）
│   │
│   ├── ui_state.rs            AppState（Arc<Mutex<>> 可共享状态）+ drain_messages + persist
│   ├── detail_fmt.rs          详情/镜像详情 → SharedString 元组格式化（format_bytes）
│   ├── ui_rows.rs             4 种行构建 + Compose 分组排序 + 网络/卷搜索过滤
│   ├── ui_update.rs           derive_update(Msg → UiUpdate)
│   ├── ui_apply.rs            apply_detail / apply_lists / refresh_lists_now（搜索排序即时重建）
│   │
│   ├── callbacks.rs           回调绑定中枢 + restore_config（含启动焦点引导 boot_tick）
│   ├── callbacks_container.rs 容器启停/重启/删除/logs/inspect（AppActions global）
│   ├── callbacks_image.rs     镜像删除（AppActions global）
│   │
│   └── docker/
│       ├── mod.rs             connect() + pub use 重导出
│       ├── containers.rs      list / start / stop / restart / remove
│       ├── images.rs          list / pull（含进度闭包）/ remove
│       ├── logs.rs            inspect_container / follow_logs（Stream → tx.send）
│       ├── stats.rs           fetch_stats + compute_cpu_delta（双采样 CPU%）
│       ├── netvol.rs          list_networks + list_volumes
│       ├── image_details.rs   inspect_image → ImageDetail
│       └── system.rs          cleanup prune
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po   gettext 翻译（编译期嵌入）
│
└── ui/                        Slint 层（所有 ≤ 300 行）
    ├── app.slint              MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 布局组装
    ├── theme.slint            浅/深双主题 22 语义色令牌 + theme_mode 三态
    ├── models.slint           ContainerRow / ImageRow / LogLineRow / NetworkRow / VolumeRow struct
    ├── top-bar.slint          TopBar：logo + 连接状态 + 搜索框 + 4 按钮
    ├── sub-bar.slint          SubBar：筛选 chips + 排序 + 4 视图 tabs
    ├── cards.slint            四种资源卡片网格
    ├── foot-bar.slint         底部 8 项资源概览
    ├── batch-bar.slint        批量操作浮动栏
    ├── drawer.slint           右侧抽屉：详情/日志/环境 三 Tab
    ├── detail-panel.slint     DetailRow + 详情 kv section（drawer 复用）
    ├── dialogs.slint          PullDialog + CleanupDialog 弹窗
    ├── delegates.slint        容器/镜像表格行（RowGroup/RowContainer/RowImage）
    ├── network-views.slint    网络/卷表格行（RowNetwork/RowVolume）
    ├── headers.slint          各视图表头（列宽严格对齐 delegates）
    └── components.slint       IconBtn / Dot / CheckCell 等原子组件
```

### 依赖方向（严格单向，无循环）

```
main.rs
  ├── consumer.rs    → ui_state + ui_rows + ui_update + ui_apply + slint_gen
  ├── bootstrap.rs   → docker::{fetch_stats, inspect_container, list_*}
  ├── ui_state.rs    → models.rs + docker::{compute_cpu_delta, inspect_container, list_*}
  ├── ui_apply.rs    → slint_gen (MainWindow) + ui_state::DetailTuple + ui_rows
  ├── callbacks.rs   → slint_gen + ui_state::SharedState + docker + config
  ├── callbacks_container.rs / callbacks_image.rs → 同 callbacks.rs
  └── docker/        → 纯 async 层，不反依赖任何上层模块

ui/*.slint           app.slint 是唯一组装入口，import 其余子文件，
                     子文件之间不互相 import（避免 Slint 跨文件 global 兼容性坑）
```

## 2. 线程模型（三线程架构）

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

## 3. 消息流（Msg 变体一览）

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

## 4. Slint UI 模块交互

16 个 `.slint` 文件——app.slint 是唯一组装入口，子文件间**不互相 import**。

### 交互总图

```
                    app.slint（组装层 + 布局 + global-shortcuts 快捷键）
                 ╱  ╱  ╱  ╱  ╱  ╲  ╲  ╲  ╲  ╲
    top-bar   sub-bar  cards  foot-bar  batch-bar  drawer  dialogs
    (顶栏)     (筛选)  (卡片)   (底栏)    (批量栏)  (抽屉)  (弹窗)
        │         │       │        │
        └────┬────┴───┬───┴────────┘
             ▼        ▼
      components.slint  theme.slint（全局设计令牌，全部子文件引用）
             │        ▼
      delegates.slint（容器/镜像行）  network-views.slint（网络/卷行）
      headers.slint（表头）          detail-panel.slint（drawer 详情 section）
             │
             ▼
      models.slint（ContainerRow / ImageRow / LogLineRow / NetworkRow / VolumeRow 数据桥）
```

### 交互模式

#### ① models.slint → Rust：property 数据注入（不变）

```
models.slint struct           Rust pump_and_apply 构造
┌──────────────────────────┐  ┌─────────────────────────┐
│ ContainerRow { id, ... } │  │ let rows = state.lock() │
│ ImageRow    { id, ... } │  │   .containers.iter()    │
│ LogLineRow  { text }    │  │   .map(build_row)       │
│ NetworkRow  { ... }     │  │   .collect::<Vec<_>>(); │
│ VolumeRow   { ... }     │  │ ui_apply::apply_lists() │
└──────────────────────────┘  └─────────────────────────┘
```

`include_modules!()` 在 `slint_gen.rs` 一次展开，其余 Rust 模块通过 `use crate::slint_gen::*` 导入。

#### ② 卡片/行组件 ← app.slint：callback 属性注入

卡片与行组件不 import AppActions global——只声明 `callback toggle(string)` 等。
app.slint / cards.slint 组装处注入实现转发到 AppActions。

#### ③ top-bar / sub-bar ↔ app.slint：`<=>` 双向绑定

`search_q <=> root.search_q` / `docker_ok <=> root.docker_ok` / `view <=> root.view`

#### ④ Drawer：条件渲染 + 自绘 Tab

`if root.drawer_open : Drawer {}` 条件渲染；抽屉内详情/日志/环境三 Tab 自绘 TabBar 切换。
**Esc 关闭弹层后焦点悬空**——app.slint Esc 分支末尾调 `global-shortcuts.focus()` 归还焦点。

### Slint 文件清单速查

| 文件 | 行数 | 职责 | import 谁 |
|------|------|------|-----------|
| `app.slint` | 267 | MainWindow + global-shortcuts FocusScope + 布局组装 | theme / top-bar / sub-bar / batch-bar / cards / foot-bar / drawer / dialogs / models |
| `theme.slint` | ~120 | 浅/深双主题 22 语义色 + 字号/间距令牌，theme_mode 三态 | 无 |
| `models.slint` | ~40 | 5 种 Row struct | 无 |
| `top-bar.slint` | ~100 | TopBar（搜索框 + 4 按钮 + focus_search） | theme / components |
| `sub-bar.slint` | ~150 | 筛选 chips + 排序 + 4 视图 tabs | theme / components |
| `cards.slint` | 180 | 四种资源卡片 | theme / models |
| `foot-bar.slint` | ~90 | 底部 8 项概览 | theme |
| `batch-bar.slint` | ~60 | 批量操作浮动栏 | theme / components |
| `drawer.slint` | 223 | 右侧抽屉三 Tab | theme / detail-panel / components |
| `detail-panel.slint` | ~100 | DetailRow + 详情 kv section | theme |
| `dialogs.slint` | 172 | PullDialog / CleanupDialog | theme / components |
| `delegates.slint` | 224 | RowGroup / RowContainer / RowImage | theme / components |
| `network-views.slint` | 93 | RowNetwork / RowVolume | theme |
| `headers.slint` | ~60 | 各视图表头 | theme |
| `components.slint` | 294 | IconBtn / Dot / CheckCell 等原子组件 | theme |

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

所有源码文件 ≤ 300 行（当前最大 components.slint 294 / callbacks.rs 280）。

**Slint 拆分原则：**
- app.slint 做组装层——只放 MainWindow 骨架 + global-shortcuts FocusScope + import 语句 + 布局 markup
- 子文件之间不互相 import——避免循环依赖和跨文件 global 可见性问题
- 卡片/行组件用 callback 属性注入代替直接 import AppActions global，绕开 Rust 侧 glob import 兼容性坑
- 顶栏/筛选栏用 in-out 属性 `<=>` 双向绑定——数据流向一眼可见
- **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸

**Rust 拆分原则：**
- 组装层分离：main.rs（i18n → AppState → MainWindow → 各模块启动 → ui.run() → 优雅退出）纯组装不写业务
- **consumer.rs 独占 Receiver + pump_and_apply 两阶段泵**——3s 刷新 interval 也在 consumer.rs；6s stats 与首屏加载在 bootstrap.rs
- **详情字段格式化抽成 detail_fmt.rs**——ui_state.rs 只管状态变更与消息排空
- **derive_update match 抽成独立 ui_update.rs**——update 推导单独一个文件
- 回调按实体分：callbacks.rs（顶栏+弹窗+配置恢复）+ callbacks_container.rs + callbacks_image.rs
- 类型集中入口：`slint_gen.rs` 只做 `include_modules!()` 一次展开
- **所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）**——主线程不持有独立 tokio runtime
- **所有 Docker 操作结果都走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新
