# AGENTS.md — docker-panel 开发协作入口

> 给 AI 编码助手看的项目速查手册。首次进入仓库请先读此文件与规则配置层 [docs/agents-rules.md](docs/agents-rules.md)。

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

## 规则地图与红线约束（外部配置层注入）

> 行为规则的权威定义在独立配置层 [docs/agents-rules.md](docs/agents-rules.md)（规则全文 + 四阶段门禁 + 冲突记录），本文件只保留**规则地图**与**红线摘要**，规则与执行主体解耦。规则间冲突按 R3（架构优先）裁决。

| ID | 规则 | 维度 | 权威定义 |
|----|------|------|----------|
| R0 | 项目名禁止出现废弃旧名 | 红线 | 本文件顶部硬约束 |
| R1 | UI 验证只能通过 Slint MCP | 红线 | 本文件顶部硬约束 |
| R2 | 去头部/注释后正文 >300 行必须拆分 | 红线 | 本文件顶部硬约束 |
| R3 | 架构设计优先级绝对高于代码实现 | 红线 | agents-rules.md §2.1 |
| R4 | 复用率 ≥80%，不足触发二次校验 | 红线 | agents-rules.md §2.2 |
| R5 | 结构设计强制规范 | 红线 | agents-rules.md §2.3 |
| R6 | 冲突无条件优先架构/结构 + 自动记录 | 红线 | agents-rules.md §2.4 |
| R7 | 锁纪律 / Receiver 独占 / spawn 走 handle | 红线 | 本文件 代码规范 #2-5 |
| R8 | 文档同步触发表 | 流程 | agents-rules.md §6 |
| R9 | 版本号三同步 | 流程 | docs/git-flow.md |
| R10 | 命名 / 错误处理 / Slint 隔离等惯例 | 惯例 | 本文件 代码规范 #6-9 |

全链路四阶段门禁（S1 需求解析 → S2 方案生成 → S3 代码输出 → S4 最终校验）检查单见 agents-rules.md §3，**任一阶段不达标不得进入下一阶段**。

## 项目是什么

**docker-panel** 是一款极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、纯软件渲染器——零 GPU 依赖、零 Electron、零运行时。当前版本 v0.2.3（Phase 2+ 已交付：卡片化四视图 + 右侧抽屉详情/日志/环境 + 网络/卷视图与容器计数 + 清理/拉取弹窗 + i18n + 配置持久化 + 全局快捷键）。

## 技术栈（精确版本 + 版本关键备注）

> **写代码时必须按下方版本的 API 写，禁止凭旧版本记忆。**
> 不确定的 API 用法：Slint 查本文件「Slint 1.18 已验证的限制」，bollard 查 docs/arch/docker-bollard.md。

| 组件 | 精确版本 | Feature 开关 | 版本关键备注（按此版本 API 写） |
|------|----------|--------------|----------------------------------|
| Rust | stable | edition 2021 | — |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std` | ⚠️ **已验证：strict 模式（无 compat-1-2）编译通过**。版本特有 API 限制见下方「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency | `slint_build::compile("ui/app.slint")` |
| bollard | **0.21** | — | ⚠️ 版本特有类型坑见 [docs/arch/docker-bollard.md](docs/arch/docker-bollard.md)（唯一权威源） |
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

## 模块地图（目录职责速查）

逐文件注释见 README「项目结构」（权威、注释齐全），此处只保留目录级职责与关键约束：

| 目录/文件 | 职责 |
|-----------|------|
| `ui/` | Slint 声明层（15 文件按职责拆分）：app（MainWindow 唯一组装入口）/ theme（设计令牌）/ models / top-bar / sub-bar / cards + card-{container,image,netvol}（三视图卡片）/ foot-bar / batch-bar / drawer / dialogs / menu-layers（浮动菜单层）/ **components（原子组件库，R4 复用检索起点）** |
| `src/` | Rust 逻辑层：main（组装）/ slint_gen（include_modules 唯一入口）/ i18n（含 `trfp` 复数翻译）/ config（持久化）/ consumer（Msg 泵）/ bootstrap（定时采样）/ models / ui_state（AppState + `follow_id` 日志流 + `cleanup_counts`）/ render（CachedModel 差量同步）/ ui_update / ui_apply / batch / callbacks{,_container,_drawer,_image,_netvol} / detail_fmt/{container,image,netvol} / ui_rows/{containers,images,netvol} / docker/{mod,containers,images,logs,stats,netvol,image_details,system} |
| `translations/` | gettext zh_CN（编译期嵌入，按 locale 选择） |
| `docs/` | architecture.md（总纲：技术栈/模块地图/文件规范）+ arch/（threading 线程与消息 · ui-interaction Slint 交互 · docker-bollard Docker 与 bollard 坑条）· agents-rules.md（规则配置层）· git-flow.md（发版规范） |
| `packaging/` | debian/control（版本三同步之一）+ desktop 入口 + svg 图标 |
| 根 | Cargo.toml（依赖 & release profile）/ build.rs（slint_build 编译 UI + bundled translations）/ CHANGELOG.md |

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

**关键设计**：Receiver 独占不进 Mutex（死锁唯一保证）· Slint Timer 已废弃（软件渲染器 + MCP 模式不触发，改 tokio interval）· pump_and_apply 两阶段铁律（锁内 drain + build → drop lock → 锁外 `invoke_from_event_loop`，闭包绝不碰 state）。完整推导与代码示例见 [arch/threading.md](docs/arch/threading.md)。

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

1. **单文件 ≤ 300 行**——统计口径见顶部硬约束 R2（去除头部与注释后计正文）。当前所有文件均合规（最大 callbacks.rs 280 / models.rs 273）。
2. **Mutex 锁持有最小化**——锁内**绝不**做任何可能耗时的操作（await、invoke_from_event_loop、长时间迭代）。锁内只做 clone / 取字段 / spawn 启动。
3. **Receiver 不进 Mutex**——`mpsc::Receiver` 由 consumer 线程独占所有权，绝不能放回 `AppState`。
4. **所有 spawn 都走 state.rt**——`let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(async move { ... });`
5. **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。
6. **Slint 子文件不互相 import**——避免循环依赖和跨文件 global 兼容性坑。delegates 用 callback 属性注入替代直接 import AppActions global。
7. **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸，实例化处硬设。
8. **命名**：Rust 常规 snake_case / PascalCase；Slint 全局函数 snake_case，结构体 PascalCase。
9. **错误处理**：Docker API 错误统一转为 `Result<T, String>` 上抛，UI 层展示中文文本。

## 构建命令

```bash
# 开发调试
cargo run

# 发布构建（体积最小化 profile）
cargo build --release
./target/release/docker-panel

# MCP 截图验收：命令见顶部「硬约束：UI 验证只能通过 Slint MCP」（不在此重复）

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

> 已迁移至规则配置层 [docs/agents-rules.md](docs/agents-rules.md) §6（触发事件 → 文档清单与职责 → Wiki 流程 → 双份同步原则）。代码变更后按该节执行同步。
> 提交前快速校验：`grep -ri dvkit .`（零残留）+ 核对 README 项目结构树 + 核对各文件行数（R2 口径）。

## 当前已知限制

- **无 Docker Compose 导入导出、无定时清理**（后续 phase 再加）
- **镜像行不支持多选批量**（当前只有容器 Tab 支持 CheckBox 多选）

## 文档导航

- [规则配置层](docs/agents-rules.md) — 规则地图、红线约束、四阶段门禁、冲突记录、文档同步规范
- [架构设计](docs/architecture.md) — 总纲 + arch/ 主题文档（线程/消息/回调、Slint 交互、Docker/bollard 坑条）
- [Git Flow 规范](docs/git-flow.md)
- [更新日志](CHANGELOG.md)
