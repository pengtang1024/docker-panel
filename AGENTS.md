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

**docker-panel** 是一款极简 Docker Engine 桌面管理面板。Rust 单二进制、Slint 原生 GUI、纯软件渲染器——零 GPU 依赖、零 Electron / Node 运行时。当前版本 v0.5.0（Phase 2+ 已交付：**四个视图统一列表（容器密集台账 + 台账内联 Compose 树 + 镜像/网络/卷表格列表，表头可排序）** + 分组带组级操作与分组合并日志 + 右侧抽屉详情/日志/环境 + 清理/拉取弹窗 + i18n + 配置持久化 + 全局快捷键 + 虚拟化日志面板）。

## 技术栈（精确版本 + 版本关键备注）

> **写代码时必须按下方版本的 API 写，禁止凭旧版本记忆。**
> 不确定的 API 用法：Slint 查本文件「Slint 1.18 已验证的限制」，bollard 查 docs/arch/docker-bollard.md。

| 组件 | 精确版本 | Feature 开关 | 版本关键备注（按此版本 API 写） |
|------|----------|--------------|----------------------------------|
| Rust | stable | edition 2021 | — |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std` | ⚠️ **已验证：strict 模式（无 compat-1-2）编译通过**。版本特有 API 限制见下方「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency | `slint_build::compile("ui/app.slint")` |
| Slint Material Components | **1.1.0** | 项目内 `material-1.1.0/`（MIT） | 通过 build.rs `with_library_paths` 配置 `@material`。**实际只用到 ScrollView / ListView**（drawer 日志 + 详情面板）+ **MaterialPalette 色板**；**未使用 Modal / Dialog 内置组件**——项目自己的 DialogShell 是手搓 `inherits Rectangle`（坑条见 Slint 限制表）。色板经 `ui/material-theme.slint` 映射到项目设计令牌 |
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

#### Dialog 弹窗踩坑（**DialogShell 已按正确写法固化，后续 Dialog 子类直接继承即可**）

> DialogShell（`components-dialog.slint`）是**手搓**的 `inherits Rectangle`——**没有**继承 Material Modal
> （Material Modal 是裸组件，Window 层级不给 100% 尺寸；它的 `background_layer` 语义也与项目遮罩冲突）。
> 以下是踩过的真实坑条。

| 坑 | 正确做法 |
|----|----------|
| **DialogShell 必须 inherits Rectangle** | Material Modal 是裸组件、没有默认 `parent.width`；项目自己继承 Rectangle 并显式 `width: 100%; height: 100%` |
| **TouchArea 的 `width/height: 100%` 对 Rectangle 父级不可靠** | 遮罩 TouchArea 必须显式写 `root.width` / `root.height` |
| **子类只能在父类声明的层级里覆盖 slot** | 用 `:=` 覆盖的元素必须与父类声明同层级，否则 Slint 会在根级新建实例；所以父类只提供 card 容器 + `@children`，子类写完整 Header/Body/Footer |
| **一个组件只能有一个 `@children` 入口** | 父类不能再开第二个 `@children` |
| **遮罩 TouchArea 会吞掉所有子内容点击** | card 内部必须再包一层 TouchArea，否则点按钮/CheckBox 会穿透触发遮罩关闭 |
| **`TouchArea { layout := VerticalLayout { ... } }` 的 `layout.min_width/min_height` 在 Slint 1.18 下不可靠** | 放弃 auto-size 模式，改用硬值 `width: 420px; height: root.card_height;` 显式居中 |
| **VerticalLayout 子元素显式 `height` 被忽略** | 用 `min-height: 60px`（Header/Footer）+ `vertical-stretch` 分配剩余空间（Body `vertical-stretch: 1`）。**永远不要在 VerticalLayout 子元素写 `height:`！** |
| **Dialog 内三段 VerticalLayout 必须显式 `height: 100%`** | `VerticalLayout { width:100%; height:100%; ... Header/min-height:60 ... Body/stretch:1 ... Footer/min-height:56 }` |

#### M3（@material）接项目主题（**已实现：`ui/material-theme.slint` + `ui/app.slint`**）

> 目标：`@material` 的 ScrollView / ListView（drawer 日志 + 详情面板 + 各资源列表）与 std-widgets(fluent) 滚动条，
> 全部跟随**应用主题**（auto/light/dark）而非 OS 主题，颜色全部由项目 `Theme.*` 令牌派生。

| 坑 | 正确做法 |
|----|----------|
| **全局属性赋值语句不合法** | `Palette.color-scheme := ...` / `MaterialPalette.schemes := {...}` 都是 Parse error（`Palette` 被 import 后是**类型名**，表达式里要写 `Palette.color-scheme`） |
| **不能 `changed <全局属性> =>`** | 只能盯本组件属性（写 `changed theme_mode =>`，**无 `root.` 前缀**）；一个组件只能有一个 `init` |
| **必须经本组件属性中转** | `in-out property <ColorScheme> color_scheme <=> Palette.color-scheme;` + `in-out property <MaterialSchemes> m3_schemes <=> MaterialPalette.schemes;`，再在 `init` / `changed theme_mode` / `changed system_dark` 里赋值灌色板 |
| **赋值不会打断 `<=>` 绑定**（已查源码坐实） | `i-slint-core-1.18.1/properties.rs:1029` `Property::set` → `if !previous_binding_intercepted { remove_binding() }`；`properties/two_way_binding.rs:21` `TwoWayBinding::intercept_set` 返回 `true` 并把值同步到对端 → **赋值照常透传且绑定保留**，主题切换实时跟随 |
| **light/dark 两个槽位填同一份映射** | `Theme.*` 本身已按 `is_dark` 解析，真正决定用哪套的是 `Palette.color-scheme`；两槽同填不冲突 |
| **M3 ScrollBar 滑块自动隐藏，像素验收要用 MCP 悬停** | 滑块 `opacity` = 滚动中 40% / 悬停 70% / 静止 1.5s 后淡出；采样必须用 MCP `move_pointer` 悬停到滚动条上再 `take_screenshot`，否则采样全 miss |

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
| `ui/` | Slint 声明层（36 文件按职责拆分）：app（MainWindow 唯一组装入口）/ app-actions（UI→Rust 回调 global）/ theme（设计令牌，三态主题 auto/light/dark）/ **material-theme（M3 色板映射：Theme.* → MaterialScheme，53 个角色全覆盖）**/ models / top-bar / sub-bar / **main-area（主列表工作区：SubBar + BatchBar + ListArea）**/ **list-area（资源列表区：容器视图走台账，镜像/网络/卷走表格列表）**/ **ledger（容器台账区：列宽统一计算 + 表头 + 虚拟化行）**/ **ledger-row（30px 台账行 delegate）**/ **ledger-tree-row（28px 分组带 delegate：项目带 / 服务带，通栏 + 组内资源汇总 + 组操作按钮）**/ **ledger-band（概览带：4 计数 + CPU TOP4）**/ **ledger-cols（列宽契约 global，表头与行共用）**/ **{image,network,volume}-list（三个资源表格列表：表头可点排序 + 30px 行）**/ **components-list（列表共用基件：ListCols 列宽契约 + ListHeadCell 表头单元格）**/ skel（骨架屏占位）/ foot-bar / batch-bar / drawer（`log_only` 模式承载分组合并日志）+ drawer-detail-row / **drawer-log-row（日志行 delegate，虚拟化 ListView 的一行 = 一条日志）** / dialogs（PullDialog/CleanupDialog，继承 DialogShell 基类）/ menu-layers（浮动菜单层）/ **overlays（窗口级浮层：排序/卡片/分组菜单 + 弹窗 + TipLayer，声明在内容之后 → 最上层）**/ **components-dialog（DialogShell 基类，手搓 inherits Rectangle，坑条见 Slint 限制表）** / **components（原子组件库，9 个按职能拆分：interact 交互 / nav 导航 / form 表单 / chip 标签 / align 对齐 / state 状态 / empty 空态 / feedback 反馈 / dialog 弹窗基类）** |
| `src/` | Rust 逻辑层：main（组装）/ slint_gen（include_modules 唯一入口）/ i18n（含 `trfp` 复数翻译）/ config（持久化）/ consumer（分批 drain + 120ms UI 节流 + 3s 刷新）/ bootstrap（stats/卷占用/首屏/主题监听）/ models（含 `ResourceSort` 资源列表排序状态）/ ui_state（AppState + 刷新去重/超时 + 日志流状态字段 + `cleanup_counts` + `compose_collapsed`）/ **ui_state/follow_state（日志跟随状态机：单容器 / 分组合并日志建流、Clear、幂等）** / **ui_state/log_sync（日志窗口裁剪 + UI 行模型同步账 `LogSyncPlan`）**/ **ui_state/config_io（AppState ↔ config.json 读写）**/ follow_runtime（日志流运行时：建流（单流 / 分组合并多流共享取消信号）/退避重连/探针判定/保活降级）/ log_status（日志流提示文案唯一定义处，3 条 i18n key）/ render（CachedModel 差量同步 + 日志行模型 Append/Reset）/ ui_update（DrainResult + 有上限 drain + Msg 聚合）/ ui_apply（抽屉/列表灌入 + `clear_log_panel` / `clear_log_status` / `status_toast`）/ batch / callbacks{,_container,_drawer,_image,_netvol,_compose,_sort} / detail_fmt/{container,image,netvol}（详情格式化）/ ui_rows/{containers（台账行：筛选/排序/归一化比例）,ledger（概览带 + 台账入口）,compose_tree（台账内联树行：项目带/服务带 + 容器行）,group_totals（分组资源汇总：组内成员之和）,images,netvol（含排序）}（行构建）/ docker/{mod,containers,images,logs,log_types,stats,netvol,image_details,container_detail,system,raw_http（裸 HTTP：bollard 未建模的接口）} |
| `translations/` | gettext zh_CN（编译期嵌入二进制，`build.rs` 通过 `with_bundled_translations` 打） |
| `docs/` | architecture.md（总纲：技术栈/模块地图/文件规范）+ arch/（threading · ui-interaction · list-layout · docker-bollard）· agents-rules.md（规则配置层）· git-flow.md（发版规范） |
| `packaging/` | debian/control（版本三同步之一）+ **debian/copyright（DEP-5 许可清单）** + desktop 入口 + svg 图标 |
| 根 | Cargo.toml（依赖 & release profile & `license = "MIT"`）/ **deny.toml（cargo-deny 许可证 allow-list）** / build.rs（slint_build 编译 UI + bundled translations）/ CHANGELOG.md / **LICENSE（自有 MIT）/ LICENSING.md（Slint 许可选择）/ THIRD-PARTY-NOTICES.md + licenses/（第三方通知）** / material-1.1.0（Slint Material，MIT） |

## 线程模型（三线程架构，已实现优雅退出）

```
主线程 (ui.run() 阻塞)       consumer 线程               tokio runtime (multi-thread)
┌───────────────────┐      ┌────────────────────┐      ┌──────────────────┐
│ 3s tokio interval ┼──lock▶│ recv_timeout(≤100ms)│      │ 3s refresh join!  │
│ 6s stats interval ┼──lock▶│ 分批 drain ≤2048/批 │      │ 6s stats join_all │
│ UI 回调 ───────────┼──lock▶│ pending.merge       │      │ 30s volume df     │
│                     │      │ 120ms 节流 apply    │      │ bollard API 调用   │
│                     │      │ 只重建变化区域       │      │ tx.send(Msg)       │
│                     │      │ drop lock ─────────┼─invoke▶│ follow_logs 流     │
│                     │      │   _from_event_loop  │      │                    │
└───────────────────┘      └────────────────────┘      └──────────────────┘
     Mutex<AppState>              mpsc::Receiver<Msg>（独占，不进 Mutex）
     + shutdown: AtomicBool
```

**关键设计**：Receiver 独占不进 Mutex（死锁唯一保证）· Slint Timer 已废弃（软件渲染器 + MCP 模式下 Slint Timer 不触发，改 tokio interval）· 两阶段铁律（锁内分批 drain + 只 build 变化区域 → drop lock → 锁外 `invoke_from_event_loop`，闭包绝不碰 state）· 日志洪峰防护（单批 ≤2048、UI 最多每 120ms 应用一次、纯日志不重算列表/抽屉、日志行模型只追加不重建）· Docker 调用 10s 超时 + 3s 刷新 in-flight 去重。完整推导与代码示例见 [arch/threading.md](docs/arch/threading.md)。

## 消息类型（`models::Msg`）

| 变体 | 来源 | UI 行为 |
|------|------|---------|
| `Containers(Result<Vec>)` | 3s tokio interval / 手动刷新 | 替换容器列表（保留旧 stats） |
| `Images(Result<Vec>)` | 3s interval | 替换镜像列表 + 后台补查层数 |
| **`Networks(Result<Vec>)`** | 3s interval / 手动刷新 | 替换网络列表（网络视图） |
| **`Volumes(Result<Vec>)`** | 3s interval / 手动刷新 | 替换卷列表（卷视图） |
| `PullProgress(String)` | pull_image 进度闭包 | 状态文本更新 |
| `PullDone(Result)` | pull_image 结束 | 关弹窗 + 标记 `refresh_now` |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本 + 标记 `refresh_now`（apply_ui 统一 `s.refresh()`） |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 标记 `refresh_now` |
| `ContainerDetail(Result<ContainerDetail>)` | inspect_container | 替换 `AppState.detail` → Drawer 详情 Tab |
| **`LogBatch(u64, Vec<String>)`** | follow_logs Stream | `(stream_epoch, 行列表)`；不匹配当前流则丢弃 → 推入 `AppState.logs`（滚动窗口 ≤2000 行）→ 追加到 UI 日志行模型 |
| `LogStreamEnded(u64)` | 日志流结束（**单个 stream**） | 仅当前 stream_epoch 且**全部** stream 都退出（`follow_active` 无存活标志）时才清空 `follow_id` / `follow_targets` / `follow_cancel` / `follow_active`；分组合并日志有 N 条流，一条结束不代表整组结束。来源：Eof / Cancelled / 致命错误（容器已删除） |
| `LogStreamStatus(u64, String)` | follow_runtime 判定结局 | `(stream_epoch, 文本)`；仅当前流生效，写入日志面板工具栏状态位（重连中 / 已恢复 / 已停止），不占用底部状态栏。时机：「日志流中断，正在重连…」= 首次进入可恢复重连；「日志流已恢复连接」= 重连后收到首行日志 / Idle 结局 / 探针由不可达跃迁为可达（三者均要求此前提示过重连中）；「日志流已停止」= 非取消的结束 |
| `ImageDetail(Result<ImageDetail>)` | inspect_image | 替换 `AppState.image_detail` → 镜像抽屉 |
| `ImageLayers(Vec<(String, usize)>)` | 镜像层数回填 | 更新 `image_layers` 缓存 |
| `VolumeSizes(HashMap<String, u64>)` | 30s `/system/df` | 更新卷磁盘占用列 |
| **`StatsBatch(Vec<(String, ContainerStatsSnapshot)>)`** | 6s tokio interval + 首屏加载补采一轮 | 整轮采样打包成一条消息 → 写入 `AppState.stats_prev`（双采样缓冲）→ `compute_cpu_delta` → 容器行 CPU%/内存/网络列（容器行数值同帧更新，不阶梯跳动）；本地无 prev 时用 Docker 同帧的 `precpu_stats` 兜底 → **首帧即真实 CPU%** |

## 代码规范

1. **单文件 ≤ 300 行**——统计口径见顶部硬约束 R2（去除头部与注释后计正文）。当前所有文件均合规。
2. **Mutex 锁持有最小化**——锁内**绝不**做任何可能耗时的操作（await、invoke_from_event_loop、长时间迭代）。锁内只做 clone / 取字段 / spawn 启动。
3. **Receiver 不进 Mutex**——`mpsc::Receiver` 由 consumer 线程独占所有权，绝不能放回 `AppState`。
4. **所有 spawn 都走 state.rt**——`let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(async move { ... });`
5. **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。
6. **Slint 子文件不互相 import**——避免循环依赖和跨文件 global 兼容性坑。delegates 用 callback 属性注入替代直接 import AppActions global。
7. **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸，实例化处硬设。
8. **命名**：Rust 常规 snake_case / PascalCase；Slint 全局函数 snake_case，结构体 PascalCase。
9. **错误处理**：Docker API 错误统一转为 `Result<T, String>` 上抛，UI 层展示中文文本。
10. **可重试 Docker 调用必须走 `docker::with_timeout`**（列表 / stats / inspect / 卷占用）；3s 刷新依赖 `refresh_inflight` 去重，禁止在 interval 里无界叠加任务。
11. **高频 Msg 不得逐条 invoke**——统一走 `consumer` 的分批 drain + 120ms 节流；UI 只重建 `DrainResult` 标记的变化区域（容器/镜像/网络/卷各自独立标记），日志走行模型追加。
12. **可能永久阻塞的读必须可被 shutdown 唤醒**——例如 dconf reader 放独立线程，监控线程用 `recv_timeout` 轮询；禁止在会被 join 的线程里做无超时的阻塞读。
13. **日志流重连必须退避且永不放弃**——Docker 流异常（`LogStreamEnd::Error`）按 `retry_delay` 指数退避重连（1s→2s→4s→8s→10s 上限），禁止立即紧密重试拖垮 Docker；连续失败预算 `MAX_RETRY_ATTEMPTS = 8` 耗尽后转 30s 低频保活（`KEEPALIVE_INTERVAL`），**不得静默放弃跟随**。超过 `LOG_IDLE_TIMEOUT`（180s）无任何日志视为连接挂死，走 `KeepAlive` 分支：静默重连、**不计入失败预算**、不打扰用户。容器是否还活着一律由探针 `probe_container` 判定（`Running`/`Stopped`/`Gone`/`Unreachable`），禁止用跨流共享的 running 标志。每条流的存活标志必须独立，旧流退出不得误标新流。
14. **日志流状态提示三态**——文案唯一定义处是 `src/log_status.rs`（禁止内联字面量）：「日志流中断，正在重连…」/「日志流已恢复连接」/「日志流已停止」。「已恢复连接」只允许三个触发点：重连后收到首行日志、Idle 结局、探针由「不可达」跃迁为「可达」；三者均要求此前提示过「正在重连」。切换资源或 Clear 必须清掉残留提示（`ui_apply::clear_log_panel` / `clear_log_status`）。
15. **日志行模型只追加不重建**——UI 日志行模型（`render::logs_sync`）是日志窗口（`AppState.logs`）的**镜像**，用「全局行流位置」对齐：起点相同 → `LogSyncPlan::Append`（只 `extend`，可见行不重排 → 不卡）；起点不同（窗口裁剪过前段 / 换了流）→ `LogSyncPlan::Reset`（整表替换）。**禁止**逐行丢最旧行（整表索引每行错位一次，ListView 每次都把可见行标脏重排 = 旧卡顿主因）；裁剪必须分块（`LOG_ROWS_MAX = 2000` → 保留 `LOG_ROWS_KEEP = 1000`）。同步账实现见 `src/ui_state/log_sync.rs`。
16. **状态栏 toast 必须自动恢复**——回调直接写状态栏（复制日志 / 清空日志 / 复制字段值）一律走 `ui_apply::status_toast`，2s 后恢复「就绪」；期间若被其它消息改过则不覆盖（避免旧定时器盖掉新结果）。
17. **Wayland 必须显式设置 xdg app_id**——`main.rs` 在 `MainWindow::new()` 之后、`ui.run()` 之前调用 `slint::set_xdg_app_id("docker-panel")`（仅当 `WAYLAND_DISPLAY` 存在时）。不设置时 toplevel 没有 app_id（Wayland trace 里查不到 `xdg_toplevel.set_app_id`），GNOME Shell 无法把窗口匹配到 `docker-panel.desktop`，**面板坞（Dock）会显示成「未识别程序」、图标退化成通用占位图**。仅在 Wayland 下设置：X11 由 winit 用 argv[0] 回退生成 `WM_CLASS`（class 段 = `docker-panel`，与 `.desktop` 的 `StartupWMClass` 一致），显式设置反而会让 class 段变空（Slint 固定传 `with_name(app_id, "")`）。
18. **UI 状态单一写入方——回调写，consumer 禁回写**——用户操作过的 Slint 属性（`sort_field` / `sort_desc` / `filter` / `view`）一律由 UI 回调独占写入：回调在 UI 线程里同步改 `AppState` → 立刻 `ui.set_xxx(...)` → 重建受影响的列表。consumer 的 3s 心跳**只刷数据、不回写这些属性**：它在锁内取快照、锁外才写 UI，期间用户改了条件时回写的就是**旧值**（排序状态与上下箭头偶发对不上、箭头滞后最多 3s）。需要防「锁内建行、锁外应用，期间条件被改」的竞态时用 `AppState::rows_epoch`：排序/筛选/搜索变更时递增，consumer 应用行模型前比对，epoch 变了就丢弃这批旧行（回调已用新条件重建过）。
19. **TouchArea 命中顺序——热区声明在布局之前**——Slint 按声明顺序做命中测试，后声明的兄弟节点先拿事件。按钮的「整块热区」TouchArea 若声明在 HorizontalLayout **之后**，会盖住布局内部自己声明的 TouchArea（死代码：点箭头只会触发外层热区）。统一写法：`StateArea` / TouchArea 在前、内容布局在后（见 `Chip` / `ViewTab` / 排序按钮）；布局里的 `Text` 无 input region，点击会穿透到下面的热区。
20. **台账列宽必须由父级统一计算后显式下发**——Slint `HorizontalLayout` 先给每个子项「首选宽度」（`Text` = 文本自然宽度），再把剩余空间按 `horizontal-stretch` 分。弹性列（名称 / 镜像）若直接写 `horizontal-stretch: 1`，文本一长列就被撑宽 → CPU / 内存列起点**逐行漂移**，表格失去扫读价值。正确写法：`LedgerArea` 算出 `name_w` / `image_w` 后显式下发给表头与行（`width: root.name_w; horizontal-stretch: 0;`）。响应式隐列（磁盘 / 端口）也由父级统一判定，表头与行共用同一组判定。验收用 MCP `get_element_tree` 逐列比对 `absolutePosition.x` 与 `size.width`，不要靠肉眼读截图。详见 [docs/arch/ui-interaction.md](docs/arch/ui-interaction.md) §3.3。
21. **排序基线一律自然升序**——`sort_containers` 各分支的比较器都必须是「小的在前」，`sort_desc` 只负责反向。曾经 CPU / 内存 / 磁盘三列的基线写成降序，导致 `sort_desc=true`（UI 上的 ↓ 箭头）反而从小到大排，与「名称」列和箭头方向全都不一致。
22. **compose 分组是分组，不是筛选器**——台账内联树（`ui_rows/compose_tree.rs` 产出的分组带 + 容器行）没有「范围筛选」概念：排序字段只决定**组内**顺序，组间按组名稳定排。`compose_collapsed`（**收起的**分组键集合）由 `callbacks_compose.rs` 独占写，consumer 只读；不在集合里 = 展开 → 新分组默认展开。概览带与筛选 chips 的计数一律取**可见容器集合**（实际渲染成行的那些容器），折叠分组后两个数字同步变小。
23. **分组带的命中顺序与几何**——分组带**通栏**（独立 delegate `kind=1/2`，不参与列宽计算），内部镜像 `ledger-row.slint` 的列结构（同一套 `LedgerCols` + 父级下发的同一组 `name_w` / `image_w` / `show_disk` / `show_ports`），使带内数值与容器行**同 x**；展开箭头落在状态点列内（按 `depth*18` 缩进），分组名热区从名称起点开始 → 两者**不重叠**，互不抢事件（规范 #19）；分组名落在 `44px + depth*18`（箭头 16px + 色点 18px + 间隙），与**同层**容器行的名称列起点逐像素对齐，层级只靠缩进表达；无 Compose 归属的容器直接排在顶层（depth=0），不另设「独立容器」分组带。
24. **资源列表的排序状态三个视图各自独立**——`ResourceSort { field, desc }`（列下标 + 方向）在 Rust → Slint 之间**打包成一个 int**（低 3 位=列下标，第 3 位=降序标记）；Slint 1.18 不支持 `%` 与 `&` 运算符，解包用 `Math.mod(x, 8)` / `x >= 8`。排序状态只由 `callbacks_sort.rs` 写（规范 #18），consumer 只读。表头方向箭头：左对齐列跟在标签后（不占布局宽度），右对齐列用**覆盖层**放在列左端（值仍右对齐，列起点不漂移）。
25. **分组带的资源汇总是「成员之和」**——分组带显示该组的 CPU / 内存 / 磁盘**总和**，落在台账的 CPU / 内存 / 磁盘列上（与容器行同 x，规范 #20）。口径：CPU / 内存只累加**运行中**成员（停止的容器行上显示「—」），磁盘累加**全部**成员（可写层占用是累计值，停止后依然成立）；一个运行成员都没有时 CPU / 内存显示「—」。汇总描述的是「这个组占了多少」→ **与组是否展开无关**，收起时数字照旧。**微条口径 = docker stats 口径**（与 `docker stats` 的两个百分比同义）：CPU = **相对单核**的百分比 / 100（1.0 = 占满一核），内存 = `mem_usage / mem_limit`（1.0 = 占满配额）。**刻意不用「视图内最大值归一」**：那种相对刻度下分组带与容器行**量纲不同**（分组是成员之和，成员是个体）→ 同一列里出现「组数字是成员的 2 倍、条却一样长」（唯一分组时组条恒 100% 满格，零信息量），成员的条还被压掉一半分辨率。绝对口径下两者同量纲（都是「占上限的比例」），且**分组带的条 = 组内成员条之和** → 父子关系自洽。**最小可见补偿** `MIN_BAR_RATIO = 0.02`（≈1px/52px 轨道）只作用于**大于 0** 的比例，且**必须在成员层做**（分组带累加补偿后的成员条）→ 否则组条会比成员条之和还短，父子反向失真；概览带 TOP4 的 240px 轨道下绝对占比本就可见，**不加补偿**（保留排名区分度）。实现见 `src/ui_rows/containers.rs`（`bar_ratio` / `mem_ratio`）与 `src/ui_rows/group_totals.rs`（`GroupTotals::cpu_ratio` / `mem_ratio`）。⚠️ 改 `ui/ledger-row.slint` 的列结构时必须同步改 `ui/ledger-tree-row.slint`，否则带与行的数值列错位。
26. **分组带 = 组级操作入口，按钮与容器行同构**——分组带的 3 个按钮（主操作 / 日志 / 更多）与容器行**一一对应**，但动作**统一作用于组内全部成员**：主按钮按「是否全部运行中」在启动全组 / 重启全组间切换（启动只动没跑的、重启只动运行中的）；日志打开**合并日志**（每个运行中成员一条流，行首 `[容器名]` 前缀，抽屉走 `log_only` 模式隐藏详情/环境 Tab；无运行成员时只提示不开抽屉）；「更多」菜单 = 停止全部 / 重启全部 / 合并日志 / 删除全部（`CardMenu.kind = 4`）。成员口径 = `ui_rows::group_members`（**当前筛选后**的容器，与台账可见行同源）→ 筛选外的容器不会被组操作误伤。合并日志的实现约束：`follow_targets` 保存 `(id, 前缀)`、多流共享同一个 `watch` 取消信号与 `stream_epoch`、`LogStreamEnded` 只在全部流退出后清状态、`clear_log_buffer` 重建**全部**成员流；多流时**不发**重连/恢复/停止提示（单路结局代表不了整组）。`SelKind::Group` 下迟到的容器 inspect 结果必须丢弃（防止覆盖组标题/Tab 模式）。
27. **底部状态栏的资源汇总由 consumer 独占写入**——`app.slint` 的 `cpu_total` / `mem_total` / `disk_total` 三个属性绑定到 `FootBar`，**必须**在 `consumer::apply_ui` 里每轮 `set`（Rust 侧不写 → 恒显示 "0%" / "0MB"）。口径 = **全部容器**（与「N/N 容器」同源，不是筛选后的可见集合）：CPU / 内存只累加**运行中**容器（停止容器的 `cpu_percent` 是停止前的残留值），磁盘累加**全部**容器（`size_rw` 是累计值）——与分组带汇总（规范 #25）同口径。
28. **排序键必须与该列的显示值同源**——台账「磁盘」列显示 `size_rw`（可写层占用），排序就必须用 `size_rw`。曾经用 `block_read + block_write` 排序，而这两个字段从未被赋值（恒 0）→ 所有容器排序键相等 → 稳定排序保持原序 → **点「磁盘」表头毫无反应**（而它恰好是默认排序字段）。字段恒 0 又无写入方的，直接删掉，别留着当陷阱。

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
3. `CHANGELOG.md` 新增版本段落（`[Unreleased]` 改写为 `## [X.Y.Z]` + 底部链接；**hotfix 同样要写**）

收尾用 `scripts/check-version.sh --tag` 一键核对三处是否一致（版本号 / CHANGELOG 段落 / 底部链接 / 最新 tag）。

## 文档同步规范

> 已迁移至规则配置层 [docs/agents-rules.md](docs/agents-rules.md) §6（触发事件 → 文档清单与职责 → Wiki 流程 → 双份同步原则）。代码变更后按该节执行同步。
> 提交前快速校验：`grep -ri dvkit .`（零残留）+ 核对 README 项目结构树 + 核对各文件行数（R2 口径）。

## 当前已知限制

- **无 Docker Compose 导入导出、无定时清理**（后续 phase 再加）
- **镜像 / 网络 / 卷行不支持多选批量**（当前只有容器视图支持 CheckBox 多选）
- **断流窗口的日志不补回**：异常断流后以 `tail=0` 从当前时刻接续，断流窗口内产生的日志会丢；要补齐需改用 timestamps + `since` 游标
- **日志窗口最多保留 2000 行**：超出后分块丢掉最旧的一段（保留 1000 行）；「复制日志」复制的是窗口内容（与面板所见一致），不是容器的全部历史日志
- **容器停止后不自动重连**：探针判定容器已停止/已删除即结束流并提示「日志流已停止」，需手动切 Tab 才会重新建流
- **静默容器的「已恢复连接」提示最迟 180s**：容器 180s 无输出走 Idle 保活分支，不计失败预算也不打扰用户；「已恢复」要等到 Idle 结局才提示

## 文档导航

- [规则配置层](docs/agents-rules.md) — 规则地图、红线约束、四阶段门禁、冲突记录、文档同步规范
- [架构设计](docs/architecture.md) — 总纲 + arch/ 主题文档（线程/消息/回调、Slint 交互、列表布局、Docker/bollard 坑条）
- [Git Flow 规范](docs/git-flow.md)
- [更新日志](CHANGELOG.md)
