# AGENTS.md — docker-panel 开发协作入口

> 给 AI 编码助手看的项目速查手册。首次进入仓库请先读此文件。

## ⚠️ 硬约束：项目名称禁止出现 dvkit

本项目唯一正确的名称是 **docker-panel**（品牌名 Docker Panel）。`dvkit` 是已废弃的旧名，**任何文件、注释、字符串、日志前缀、包名、二进制名、desktop 入口中禁止出现 dvkit 字样**。提交前必须 `grep -ri dvkit .` 确认零残留。

## ⚠️ 硬约束：UI 验证只能通过 Slint MCP

本项目运行在 Wayland 下，**系统截图工具（scrot/gnome-screenshot/maim）全部黑屏**，无法用于 UI 验收。查看 UI 状态必须使用 Slint 内置 MCP 服务器：

```bash
# 1. 构建带 mcp feature 的版本
cargo build --release --features slint/mcp

# 2. 启动（SLINT_MCP_PORT 指定端口）
SLINT_EMIT_DEBUG_INFO=1 SLINT_MCP_PORT=8080 ./target/release/docker-panel &

# 3. 截图（通过 MCP HTTP 接口）
curl -s -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"take_screenshot","arguments":{"windowHandle":{"generation":"1","index":"1"},"imageMimeType":"image/png"}}}'
```

Slint MCP 还提供 `get_element_tree` / `get_element_properties` / `click_element` 等工具，可精确检查元素几何尺寸、属性值和模拟交互。**修复 UI 问题时必须用 MCP 截图验证，不要依赖用户口述。**

Trae MCP 配置（`~/.config/Trae CN/User/mcp.json`）需包含：
```json
"slint-mcp": { "type": "http", "url": "http://127.0.0.1:8080/mcp" }
```

## ⚠️ 硬约束：正文超过 300 行必须考虑拆分

任何文件去除头部与注释后，**正文内容超过 300 行必须按职责拆分**（头部、注释均不计入行数统计）。此限制适用于源码文件与文档，单一文件承担过多职责会导致维护困难与协作冲突。提交前需审查文件长度，若超限应拆分为独立模块。

## 项目是什么

**docker-panel** 是一款极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、纯软件渲染器——零 GPU 依赖、零 Electron、零运行时。当前版本 v0.2.2（Phase 2+ 已交付：详情面板 + 实时日志流 + 镜像详情 + 网络/卷 Tab + 网络容器计数）。

## 技术栈（精确版本 + 版本关键备注）

> **写代码时必须按下方版本的 API 写，禁止凭旧版本记忆。**
> 不确定的 API 用法，查本文件下方对应技术的"版本特有 API 限制"小节。

| 组件 | 精确版本 | Feature 开关 | 版本关键备注（按此版本 API 写） |
|------|----------|--------------|----------------------------------|
| Rust | stable | edition 2021 | — |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std` | ⚠️ **已验证：strict 模式（无 compat-1-2）编译通过**。版本特有 API 限制见下方「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency | `slint_build::compile("ui/app.slint")` |
| bollard | **0.21** | — | ⚠️ 版本特有类型坑见下方「bollard 0.21 易踩坑速查」 |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features = false） | `Runtime::handle()` clone 后才能 spawn；**不**用 `current_thread` |
| futures | **0.3** | — | — |
| futures-util | **0.3** | — | 提供 `StreamExt`（`.next().await`），需显式引 |
| serde_json | **1.x** | — | 用于 bollard 内部序列化 |
| anyhow | **1.x** | — | — |
| 渲染 | — | 纯软件渲染 | 无 Vulkan/GL/WGPU 依赖，deb 包 **不依赖 libgl1** |
| 构建优化 | — | release profile | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip |

### Slint 1.18 已验证的限制

> 这些是 Slint 1.18.1 版本已验证**仍然存在**的 API 缺失 / 行为限制 / 严格语法规则。
> 写 Slint 代码时必须遵守，不能假设旧版本兼容。升级 Slint 后重新核对本节。

#### 已知 API 缺失（版本间未修复）

| 限制 | 实际行为 | 必须的替代方案 |
|------|----------|----------------|
| **TabWidget 没有 `current-tab` property** | 无法通过编程式方法切换 Tab | 只能手动点 Tab；如需自动切，改成条件渲染 `if root.show_logs { LogPanel { ... } }` 替代（本项目 `detail-log-panel.slint` 已用自绘 TabBar） |
| **ListView delegate 没有内置 MouseArea** | 直接点 delegate 的空白区域不会触发点击事件 | 行点击需要在 delegate 内部**手动声明 `callback` 属性** + 在 app.slint 注入实现 |
| **delegate 不能直接 import AppActions global** | Slint 跨文件 global 可见性有坑，Rust 侧 glob import 兼容性问题 | delegate 只声明 `callback xxx(string);`，由 app.slint 的 ListView 循环里 `xxx => { AppActions.container_toggle(row.id); }` 注入实现 |
| **TabWidget 子组件不继承 parent 尺寸** | Tab 容器不给子组件自动传 width/height | 实例化处**硬设** `width: 320px; height: 600px;` |
| **`include_modules!()` 用法** | 在一个 .rs 文件（`slint_gen.rs`）里统一展开一次 | 不要在多个 .rs 里重复写 `include_modules!()` |
| **software-renderer-systemfonts 不打包字体** | 只从系统字体读取字形，没有内嵌 CJK fallback | 依赖系统装了 Noto CJK / 文泉驿 等中文字体 |
| **`invoke_from_event_loop` 只能在主线程安全调用** | consumer 线程里用 `invoke_from_event_loop` 调度 UI 更新 | 闭包里**绝不**触碰 `Mutex<AppState>`，只读闭包外 clone 的参数 |
| **FocusScope 聚焦方法名是 `focus()`** | 没有 `force-focus`；编译错误不以 `^error` 开头（build script 里是 `slint compile failed`） | 构建后必须看完整输出末尾 `Finished` 确认成功，不能只 grep `^error` |
| **无焦点元素时 Slint 完全不派发按键** | `focus_item` 为空时 capture/bubble 都不执行，全局快捷键失效 | 根级 FocusScope 包裹全部 UI + 启动焦点引导（延时 300ms 线程投递 boot_tick；`run()` 前排队的 invoke 会被 winit 丢弃） |
| **Esc 有无条件内置 accept** | i-slint-core window.rs 对 Esc 优先做 popup 关闭逻辑，即使无焦点元素也 Accepted | 不能用"Esc 被 Accepted"证明焦点引导成功；用其它键（如 Ctrl+数字）+ 事件录制验证 |
| **条件渲染移除焦点元素后焦点悬空** | 弹窗/抽屉关闭后其内部焦点丢失，后续按键无人处理 | 关闭处主动调 `global-shortcuts.focus()` 归还焦点（app.slint Esc 分支） |
| **`select_bundled_translation` 必须在首个组件创建之后调用** | 之前调用 GLOBAL_CONTEXT 未初始化 → 恒报 `NoTranslationsBundled`（debug/release 都一样） | 放在 `MainWindow::new()` 之后；切换后 `translations_dirty` 自动重算 `@tr` 属性 |

#### 1.15+ 引入的严格语法规则（**本项目已全部修完，compat-1-18 strict 模式验证通过**）

| 规则 | 旧写法（禁止） | 正确写法 | 验证状态 |
|------|---------------|---------|----------|
| **禁止 `!expr` 前缀否定运算符** | `!root.busy`、`if (!root.x)`、`if !root.y:` | `root.busy == false`、`if (root.x == false)`、`if root.y == false:` | ✅ 已全部修复（grep 零匹配） |
| **1.18 编译期检测布局循环绑定** | `HorizontalBox { Text { height: parent.height; vertical-alignment: center; } }` — 父 HorizontalBox 高度依赖子元素，子元素又 `height: parent.height` 依赖父 → 隐式循环 | 父容器必须有硬值 height，或去掉子元素的 `height: parent.height` + `vertical-alignment` 让 Slint 自动布局 | ✅ 已全部审查：所有 HorizontalBox/VerticalBox 有硬值或子元素不参与循环 |
| **1.18 编译期检测两向绑定循环** | 双向绑定链路形成循环 → 之前运行时 panic | 检查 `<=>` 链路，避免 A `<=>` B `<=>` C `<=>` A | ✅ 已全部审查 |

### tokio 1.x 版本关键用法

| 规则 | 原因 |
|------|------|
| **所有 spawn 都必须 clone handle**：`let h = rt.handle().clone(); h.spawn(async { ... })` | `tokio::spawn` 需要 runtime handle；不能传 `&Runtime` 引用到闭包里 |
| **Runtime 必须是 `rt-multi-thread`**：`tokio = { ..., features = ["rt-multi-thread"] }` | `current_thread` 单线程 runtime 下后台任务可能无法被调度，Docker 连接会无响应 |
| **`Mutex<T>` 不要进 Mutex**：`tokio::sync::Mutex` 不要包在 `std::sync::Mutex` 里 | 容易产生嵌套死锁；当前架构 std Mutex 用于 AppState + 独立 mpsc Receiver 已足够 |
| **`mpsc::Receiver<Msg>` 由 consumer 独占**：不放在 Mutex 里 | 阻塞 `recv()` 时不能持有 state 锁，否则主线程 Timer / UI 回调全卡死 |

## 模块地图（当前实际）

```
docker-panel/
├── Cargo.toml              # 依赖 & release profile
├── Cargo.lock
├── build.rs                # slint_build::compile_with_config + with_bundled_translations("translations")
│
├── ui/                     # Slint UI 声明（纯标记，Rust 侧统一通过 slint_gen 引入）
│   ├── app.slint           # MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 顶/筛/卡/底布局 + Drawer/弹窗
│   ├── theme.slint         # 全局设计令牌（浅/深双主题 22 语义色 + 字号/间距，theme_mode 三态）
│   ├── models.slint        # ContainerRow / ImageRow / LogLineRow / NetworkRow / VolumeRow struct
│   ├── top-bar.slint       # TopBar：logo + 连接状态 + 搜索框(Ctrl+K) + 主题/刷新/清理/拉取按钮
│   ├── sub-bar.slint       # SubBar：筛选 chips + 排序按钮 + 4 视图 tabs
│   ├── cards.slint         # 卡片网格：容器/镜像/网络/卷四种卡片
│   ├── foot-bar.slint      # FootBar：底部 8 项资源概览
│   ├── batch-bar.slint     # 批量操作浮动栏（选中容器才显示）
│   ├── drawer.slint        # 右侧抽屉：详情/日志/环境 三 Tab
│   ├── detail-panel.slint  # DetailRow + 详情 kv section（drawer 复用）
│   ├── dialogs.slint       # PullDialog + CleanupDialog 弹窗
│   ├── delegates.slint     # 容器/镜像表格行（RowGroup/RowContainer/RowImage）
│   ├── network-views.slint # 网络/卷表格行（RowNetwork/RowVolume）
│   ├── headers.slint       # 各视图表头（列宽与 delegates 严格对齐）
│   ├── components.slint    # IconBtn / Dot / CheckCell 等原子组件
│   ├── prototype.html      # 视觉原型（验收对照基准）
│   └── PROTOTYPE_SPEC.md   # 原型规格说明
│
├── src/
│   ├── main.rs             # 组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs        # slint::include_modules!() 集中入口
│   ├── i18n.rs             # locale 检测（zh*→zh_CN）+ Rust 侧 t() + slint_language()
│   ├── config.rs           # AppConfig 持久化（directories + 手写 JSON 原子写）
│   ├── consumer.rs         # consumer 线程：recv_timeout Msg 泵 pump_and_apply + 3s 刷新 interval
│   ├── bootstrap.rs        # 6s stats 采样 interval + 首屏四列表加载 + inspect 首容器
│   ├── models.rs           # Msg(+Stats/Networks/Volumes) / ContainerInfo / ImageInfo / ContainerDetail / NetworkInfo / VolumeInfo
│   ├── ui_state.rs         # AppState(shutdown: AtomicBool, stats_prev: HashMap) + drain_messages + persist
│   ├── detail_fmt.rs       # 详情/镜像详情 → SharedString 元组格式化（format_bytes 也在此）
│   ├── ui_update.rs        # derive_update(Msg → UiUpdate) + UiUpdate 结构体
│   ├── ui_apply.rs         # apply_detail(18-tuple) + apply_lists + refresh_lists_now(搜索/排序即时重建)
│   ├── ui_rows.rs          # 4 种行构建 + Compose 分组排序 + 网络/卷搜索过滤
│   ├── batch.rs            # BatchState 多选状态
│   ├── callbacks.rs        # 回调绑定中枢 + restore_config（含启动焦点引导 boot_tick）
│   ├── callbacks_container.rs  # AppActions container_* 回调（toggle/restart/remove/logs/select）
│   ├── callbacks_image.rs     # AppActions image_remove 回调
│   └── docker/
│       ├── mod.rs          # connect() + pub use 重导出
│       ├── containers.rs   # list / start / stop / restart / remove
│       ├── images.rs       # list / pull（含进度回调）/ remove
│       ├── logs.rs         # inspect_container + follow_logs（Stream → tx.send）
│       ├── stats.rs        # fetch_stats + compute_cpu_delta（双采样 CPU% 算法）
│       ├── netvol.rs       # list_networks + list_volumes（bollard 0.21 API）
│       ├── image_details.rs # inspect_image → ImageDetail（镜像详情 14 字段）
│       └── system.rs       # cleanup prune
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po  # gettext 翻译（编译期嵌入，按 locale 选择）
├── .trae/specs/slint-ui-overhaul/         # UI 重构 spec（tasks.md 进度）
├── docs/
│   ├── architecture.md     # 架构设计（线程模型、消息流、模块交互、bollard 坑条）
│   └── git-flow.md         # Git Flow 分支与发版规范
├── packaging/
│   ├── debian/control      # deb 包元信息（Depends 移除 libgl1，软件渲染不需要）
│   ├── docker-panel.desktop
│   └── docker-panel.svg
└── CHANGELOG.md
```

## 线程模型（三线程架构，已实现优雅退出）

```
主线程 (ui.run() 阻塞)       consumer 线程               tokio runtime (multi-thread)
┌───────────────────┐      ┌────────────────────┐      ┌──────────────────┐
│ 3s tokio interval ┼──lock▶│ recv_timeout(200ms)┼─────▶│ 3s refresh join!  │
│ 6s stats interval ┼──lock▶│ (Timeout 检查 flag) │      │ 6s stats join_all │
│ UI 回调 ───────────┼──lock▶│ drain_messages()   │      │ bollard API 调用   │
│                     │      │ pump_and_apply      │      │ tx.send(Msg)       │
│                     │      │ 操作完成立即 s.refresh()│    │ follow_logs 流     │
│                     │      │ drop lock ──────────┼─invoke▶│                    │
│                     │      │   _from_event_loop  │      │                    │
└───────────────────┘      └────────────────────┘      └──────────────────┘
     Mutex<AppState>              mpsc::Receiver<Msg>（独占，不进 Mutex）
     + shutdown: AtomicBool
```

**关键设计**：`mpsc::Receiver<Msg>` 不在 Mutex 内部，由 consumer 线程独占。`rx.recv_timeout(200ms)` 替代阻塞 `recv()`，每次 timeout 检查 `AppState.shutdown` flag。窗口关闭 → flag=true → consumer ~250ms 内 break → join → state/runtime 自然 drop。

**Slint Timer 已废弃**：软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），全部改用 `rt.handle().clone(); rt_h.spawn(async { tokio::time::interval(...); loop { tick().await; } })`。

**pump_and_apply 两阶段铁律**：
1. 锁内：`drain_messages` + `build rows` + `build_detail_data` + 读字段 + 操作完成时 `s.refresh()`
2. `drop(state)`
3. `invoke_from_event_loop` 闭包里调 `ui_apply::apply_lists(...)` / `ui_apply::apply_detail(...)` / 其他 `ui.set_*` — **闭包里绝不碰 state**

## 消息类型（`models::Msg`）

| 变体 | 来源 | UI 行为 |
|------|------|---------|
| `Containers(Result<Vec>)` | 3s tokio interval / 手动刷新 | 替换容器列表 + 剪失效选择 |
| `Images(Result<Vec>)` | 3s interval | 替换镜像列表 + 剪失效选择 |
| **`Networks(Result<Vec>)`** | 3s interval | 替换网络列表（网络 Tab） |
| **`Volumes(Result<Vec>)`** | 3s interval | 替换卷列表（卷 Tab） |
| `PullProgress(String)` | pull_image 进度闭包 | 状态文本更新 |
| `PullDone(Result)` | pull_image 结束 | 关弹窗 + 立即刷新镜像列表 |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本 + **立即触发 s.refresh()** 刷新列表 |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 立即刷新 |
| `ContainerDetail(Result<ContainerDetail>)` | inspect_container | 替换 `AppState.detail` → DetailPanel 18 字段（含 Labels）灌入 |
| `LogLine(String)` | follow_logs Stream | 推入 `AppState.logs`（环形缓冲 cap=500）→ LogPanel ListView |
| **`Stats(String id, ContainerStatsSnapshot)`** | 6s tokio interval | 写入 `AppState.stats_prev`（双采样缓冲）→ `compute_cpu_delta` → 容器行 CPU%/内存/网络列 |

## 代码规范

1. **单文件 ≤ 300 行**——超过必须按职责拆分。当前所有文件均 ≤ 300 行（最大 callbacks.rs 280 / models.rs 273）。
2. **Mutex 锁持有最小化**——锁内**绝不**做任何可能耗时的操作（await、invoke_from_event_loop、长时间迭代）。锁内只做 clone / 取字段 / spawn 启动。
3. **Receiver 不进 Mutex**——`mpsc::Receiver` 由 consumer 线程独占所有权，绝不能放回 `AppState`。
4. **所有 spawn 都走 state.rt**——`let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(async move { ... });`
5. **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。
6. **Slint 子文件不互相 import**——避免循环依赖和跨文件 global 兼容性坑。delegates 用 callback 属性注入替代直接 import AppActions global。
7. **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸，实例化处硬设。
8. **命名**：Rust 常规 snake_case / PascalCase；Slint 全局函数 snake_case，结构体 PascalCase。
9. **错误处理**：Docker API 错误统一转为 `Result<T, String>` 上抛，UI 层展示中文文本。

## bollard 0.21 易踩坑速查

| 项目 | 实际类型 | 备注 |
|------|----------|------|
| `LogOutput` | `LogOutput::StdOut { message: Bytes }` | struct 变体，**不是** tuple |
| `ContainerStateStatusEnum` | 没有 Default | 用 `.as_ref().map(\|e\| e.to_string()).unwrap_or_default()` |
| `NetworkSettings.ports` | `Option<HashMap<String, Option<Vec<PortBinding>>>>` | 双层 Option |
| `PortBinding.host_port` | `Option<String>` | 不是 u16 |
| `LogsOptionsBuilder.tail` | 收 `&str` | 不是 `String` |
| `StatsOptionsBuilder` 路径 | `bollard::query_parameters::StatsOptionsBuilder` | **不是** `bollard::stats::*` |
| `list_networks` | 不用泛型参数 | `Some(ListNetworksOptionsBuilder::default().build())` |
| `list_volumes` | 需要 type annotation | `list_volumes(None::<ListVolumesOptions>)` |
| `Network` struct 字段 | 没有 `containers` 映射 | driver/scope/name/id 都是 `Option<String>` |
| `ContainerStatsResponse.networks` | `Option<HashMap<String, ContainerNetworkStats>>` | `rx_bytes/tx_bytes` 是 **u64**（不是 Option） |
| `docker.stats()` | 返回 Stream（不是 future） | `.next().await` 取第一帧 |
| 依赖 | `futures-util = "0.3"` | 提供 `StreamExt` / `join_all` |

## 构建命令

```bash
# 开发调试
cargo run

# 发布构建（体积最小化 profile）
cargo build --release
./target/release/docker-panel

# MCP 截图验收（必须 feature slint/mcp）
cargo build --release --features slint/mcp
SLINT_EMIT_DEBUG_INFO=1 SLINT_MCP_PORT=8080 ./target/release/docker-panel &

# 代码检查
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
```

## target/ 清理

Rust 不自动清理编译缓存，`target/` 可能膨胀到 10G+。推荐工具链：

```bash
cargo clean                       # 全清（最快释放空间）
cargo clean --release             # 只清 release（保留 debug 增量）
cargo sweep --time 7              # 细粒度清理（已全局装 cargo-sweep）
```

## 发版流程

严格走 Git Flow，详见 [docs/git-flow.md](docs/git-flow.md)。三个位置必须同步版本号：

1. `Cargo.toml` 的 `version`
2. `packaging/debian/control` 的 `Version`
3. `CHANGELOG.md` 新增版本段落

## 文档同步规范

> 代码变更后，必须判断是否需要同步文档。以下是**触发时机**和**文档清单**的对应关系。

### 触发事件 → 需要更新的文档

| 触发事件 | 需同步更新的文档 |
|----------|------------------|
| 源码文件新增 / 删除 / 重命名 | README 项目结构树、AGENTS.md 模块地图、architecture.md 模块地图 |
| Slint 文件增删（ui/*.slint） | 同上 + architecture.md 第 4 节 Slint 模块交互 |
| 依赖版本升级（slint / bollard / tokio / futures） | README 依赖表、AGENTS.md 技术栈表、architecture.md 第 1 节、Wiki Architecture |
| 架构变更（线程模型、消息流 Msg 变体、模块交互方向调整） | AGENTS.md 线程模型 + 消息类型表、architecture.md 对应章节、README 架构概览 ASCII 图、Wiki Architecture |
| 功能特性变更（新增 / 删除 / 修改用户可见功能） | CHANGELOG.md、README 功能特性、AGENTS.md 当前已知限制、Wiki Home / Usage |
| 新增 bollard 类型坑条 | AGENTS.md bollard 易踩坑速查、architecture.md 第 6 节 |
| 发版（打 tag） | CHANGELOG.md 新增版本段落、三个版本号同步（Cargo.toml / debian control / CHANGELOG）、Wiki Home 版本声明 |

### 文档清单与职责

**主仓库内（随主仓库 commit）**：

| 文件 | 职责 | 谁是权威源 |
|------|------|------------|
| `AGENTS.md` | AI 协作入口：模块地图、线程模型、消息类型、代码规范、bollard 坑条、已知限制、文档同步规范 | ✅ **本文件**——首次进仓库必须读 |
| `README.md` | 人类读者的项目概览：功能特性、项目结构树、架构概览 ASCII 图、安装使用、依赖表 | — 概览型，内容来自 AGENTS + architecture.md 的精简版 |
| `docs/architecture.md` | 完整架构设计：7 大章节的深度版（线程模型、消息流、Slint 交互图、Rust/Slint 拆分原则） | ✅ **详细权威源**——与 AGENTS.md 的架构描述需保持一致 |
| `docs/git-flow.md` | Git Flow 分支与发版规范 | — 只在发版流程变更时更新 |
| `CHANGELOG.md` | 版本变更记录，按 Keep a Changelog 格式 | ✅ 发版时必须更新 |
| `packaging/debian/control` | 版本号（与 Cargo.toml 同步） | ✅ 发版三同步之一 |

**GitHub Wiki（独立 git 仓库）**：

```bash
# 更新流程（一次性）
cd /tmp && git clone git@github.com:pengtang1024/docker-panel.wiki.git
# 修改 .md 文件
git add -A && git commit -m "docs(wiki): xxx" && git push origin HEAD:master
rm -rf /tmp/docker-panel-wiki
```

| Wiki 页面 | 对应 in-tree 文档 | 更新时机 |
|-----------|------------------|----------|
| Home.md | README + CHANGELOG | 功能增删 / 发版 |
| Install.md | README 安装章节 | 安装方式变更 / 二进制名改 |
| Usage.md | README 使用说明 + 源码 UI 变化 | 用户可见交互变更 |
| Architecture.md | docs/architecture.md + AGENTS.md 架构章节 | 架构变更（**必须与 in-tree 内容同步**） |
| FAQ.md | —（独立维护） | 发现新常见问题 / 废弃旧问题 |

### 双份同步原则

- **AGENTS.md ↔ architecture.md**：架构描述两处都有，架构变更时**两边必须同步更新**。AGENTS 放 ASCII 图 + 速查表（AI 读），architecture.md 放完整深度版（人读）。
- **in-tree ↔ Wiki**：Architecture 页与 `docs/architecture.md`、Install 页与 README 安装章节、Usage 页与 README 使用章节，**同一变更必须两处都改**，避免双源漂移。
- 提交前快速校验：`grep -ri dvkit .`（零残留）+ 读一遍 README 项目结构树（确认没有已删除文件）。

## 当前已知限制

- **无快捷键、无 Docker Compose 导入导出、无定时清理**（后续 phase 再加）
- **镜像行不支持多选批量**（当前只有容器 Tab 支持 CheckBox 多选）

## 文档导航

- [架构设计](docs/architecture.md) — 各文件交互关系、线程模型、消息流、bollard 坑条
- [Git Flow 规范](docs/git-flow.md)
- [更新日志](CHANGELOG.md)
- [AGENTS.md](AGENTS.md)（本文件）
