# AGENTS.md — docker-panel 开发协作入口

> **一句话**：AI 编码助手进入本仓库的**第一份必读**——红线 Constraints 在这里，规则全文在 [docs/agents-rules.md](docs/agents-rules.md)，架构在 [docs/architecture.md](docs/architecture.md)。动手前读完本页与规则配置层，再开始任何开发活动。

## ⚠️ 硬约束：项目名称禁止出现 dvkit

本项目唯一正确的名称是 **docker-panel**（品牌名 Docker Panel）。`dvkit` 是已废弃的旧名，**任何文件、注释、字符串、日志前缀、包名、二进制名、desktop 入口中禁止出现 dvkit 字样**。提交前必须 `grep -ri dvkit .` 确认零残留。

## ⚠️ 硬约束：UI 验证统一走 agkit 截图

本项目运行在 Wayland 下，scrot / gnome-screenshot / maim 全部黑屏。**UI 验收统一走 `agkit shot`**（wayland-screenshot 技能，按 app_id / 标题定位窗口，零弹窗）：

```bash
cargo build --release
./target/release/docker-panel &                       # 普通 release 构建即可，无需额外 feature
agkit shot --app docker-panel -o /tmp/dp.png          # 按 xdg app_id 定位并截图
agkit shot --list-apps                                # 查看已记录的固定区域
```

`--app` 定位不到时才退回 `--region WxH+X+Y`（坐标基准是**整个虚拟桌面**，双屏 3840x1080）。

**要出可放进 README / Wiki 的成品图时**（`assets/screenshots/` 四张，统一 1280x720 深色主题），按下面的配方走，能避开三类污染源（原生标题栏、GNOME dock 遮挡、桌面通知横幅）：

```bash
agkit desktop enable --yes --ttl 10m          # 限时开关 + 审计；只用 win 子命令，不注入键鼠
./target/release/docker-panel &
id=$(agkit desktop win list | sed 's/^\* //' | awk '/docker-panel/{print $1}' | head -1)
agkit desktop win resize "$id" --size 1280x752   # 先 resize，再 move（反序会被 WM 拒绝）
agkit desktop win move  "$id" --to 0,32
agkit desktop mouse move 1900 1060            # 指针移出窗口，否则残留 hover 浮层进画面
agkit shot --app docker-panel -o /tmp/raw.png
python3 -c "from PIL import Image; Image.open('/tmp/raw.png').crop((0,32,1280,752)).save('assets/screenshots/x.png')"
agkit desktop disable
```

窗口高度取 752 再裁掉顶部 32px 标题栏 → 正好 1280x720。

**成品图必须脱敏**：`assets/screenshots/` 是公开产物，不允许出现真实业务标识。容器名 / 镜像名 / 分组名一律换成中性示例值（`demo-gateway`、`demo/gateway:local`、`demo-stack`），容器 ID、PID、创建 / 启动时间、挂载路径同理；CPU 占用 TOP4 面板、抽屉标题与副标题、`详情` 页的 ID / PID / 时间 / 挂载都在范围内。做法是**同色底覆盖 + 同字号同色重绘**（PIL 按**逐区采样**的底色填矩形 + 同字号重绘，`DejaVuSans` 13px；被弹窗遮罩压暗的图要取遮罩后的实际底色），**不要用黑条或马赛克**——那会破坏列宽对齐与观感。改完逐张放大复核（至少名称列、镜像列、抽屉 ID/时间区三处），确认没有残留真实标识再提交。

截图期间临时 `gsettings set org.gnome.desktop.notifications show-banners false`（**必须用 trap 还原**）；抽屉 / 清理弹窗这类需要交互的状态用环境变量门控的**临时状态注入**（`ui_apply::open_drawer` / `cleanup_counts()` + `set_cleanup_open(true)`）构造，**不要注入键鼠事件**，截完立刻移除注入代码。

Slint MCP 已降级为**调试数据暴露**通道：只在需要读元素树 / 属性值（如逐列核对 `absolutePosition.x` 与 `size.width`、确认某个属性是否被赋值）时，临时构建 `cargo build --release --features slint/mcp` 并用 `get_element_tree` / `get_element_properties`（Trae MCP：`"slint-mcp": { "type": "http", "url": "http://127.0.0.1:8080/mcp" }`）。它不再承担截图与验收，验收一律以 `agkit shot` 的出图为准。

**修复 UI 问题必须附截图证据，不要依赖用户口述。**

## ⚠️ 硬约束：正文超过 300 行必须考虑拆分

任何文件去除头部与注释后，**正文内容超过 300 行必须按职责拆分**（头部、注释均不计入行数统计）。此限制适用于源码文件与文档，单一文件承担过多职责会导致维护困难与协作冲突。提交前需审查文件长度，若超限应拆分为独立模块。

## ⚠️ 硬约束：基准仓库的破坏性操作必须先取得确认

本项目**双平台发布：GitHub 与 Gitee 都是正式发布渠道，不存在主从或镜像关系**。两侧内容应当一致，但「一致」是目标、不是动任何一侧的许可证；出现不一致时先记录差异并报告，不得为了让两侧对齐而对任一侧做破坏性操作。

以下操作在**任何情况下**都必须先逐项说明影响并取得用户明确同意，禁止以「顺手」「为了让两站一致」「某个任务需要」作为理由自行执行：

| 操作 | 风险 |
| --- | --- |
| 删除 GitHub Release / Release 资产 / tag ref | immutable release 会把 tag 名锁成墓碑，删掉后同名 ref **永久无法重建**（实测 `tag_name was used by an immutable release`） |
| 改写历史（`filter-branch` / `filter-repo` / `commit --amend` 批量改） | 不可逆；未引用对象会被 GC |
| 对 `develop` / `master` 强推（`--force`、`--force-with-lease`） | 覆盖他人提交 |
| 修改仓库 ruleset / 分支保护 / immutable releases 设置 | 影响所有协作者与 CI |

遇到保护机制（`repository rule violations`、`Cannot update this protected ref`、`Cannot upload assets to an immutable release`）时，**停止并报告**，不要通过删除对象来绕过保护——那正是上面这些不可恢复事故的来源。

现状护栏（2026-11-16 起）：GitHub ruleset `protect-release-tags`（禁 tag 删除 / 更新 / 非快进）与 `protect-main-branches`（禁 develop / master 删除 / 非快进）；`release.yml` 使用 immutable releases，发布后资产不可补传。

## 规则地图与红线约束（外部配置层注入）

> 行为规则的权威定义在独立配置层 [docs/agents-rules.md](docs/agents-rules.md)（规则全文 + 四阶段门禁 + 冲突记录），本文件只保留**规则地图**与**红线摘要**，规则与执行主体解耦。规则间冲突按 R3（架构优先）裁决。

| ID | 规则 | 维度 | 权威定义 |
|----|------|------|----------|
| R0 | 项目名禁止出现废弃旧名 | 红线 | 本文件顶部硬约束 |
| R1 | UI 验证统一走 agkit 截图 | 红线 | 本文件顶部硬约束 |
| R2 | 去头部/注释后正文 >300 行必须拆分 | 红线 | 本文件顶部硬约束 |
| R3 | 架构设计优先级绝对高于代码实现 | 红线 | agents-rules.md §2.1 |
| R4 | 复用率 ≥80%，不足触发二次校验 | 红线 | agents-rules.md §2.2 |
| R5 | 结构设计强制规范 | 红线 | agents-rules.md §2.3 |
| R6 | 冲突无条件优先架构/结构 + 自动记录 | 红线 | agents-rules.md §2.4 |
| R7 | 锁纪律 / Receiver 独占 / spawn 走 handle | 红线 | 本文件 代码规范 #2-5 |
| R8 | 文档同步触发表 | 流程 | agents-rules.md §6 |
| R9 | 版本号四处同步（含 Cargo.lock） | 流程 | docs/git-flow.md |
| R10 | 命名 / 错误处理 / Slint 隔离等惯例 | 惯例 | 本文件 代码规范 #6-9 |

全链路四阶段门禁（S1 需求解析 → S2 方案生成 → S3 代码输出 → S4 最终校验）检查单见 agents-rules.md §3，**任一阶段不达标不得进入下一阶段**。

## 项目是什么

**docker-panel** 是一款极简 Docker Engine 桌面管理面板。Rust 单二进制、Slint 原生 GUI、纯软件渲染器——零 GPU 依赖、零 Electron / Node 运行时。当前版本 v0.7.19（Phase 2+ 已交付：**四个视图统一列表（容器密集台账 + 台账内联 Compose 树 + 镜像/网络/卷表格列表，表头可排序）** + 分组带组级操作与分组合并日志 + **应用内自动更新（清单三级回退的检查 / 下载 / SHA-256 校验 / 安装 / 重启）** + 右侧抽屉详情/日志/环境 + 清理/拉取弹窗 + i18n + 配置持久化 + 全局快捷键 + 虚拟化日志面板 + **单实例保障**）。

## 技术栈（精确版本 + 版本关键备注）

> **写代码时必须按下方版本的 API 写，禁止凭旧版本记忆。**
> 不确定的 API 用法：Slint 查本文件「Slint 1.18 已验证的限制」，bollard 查 docs/arch/docker-bollard.md。

| 组件 | 精确版本 | Feature 开关 | 版本关键备注（按此版本 API 写） |
|------|----------|--------------|----------------------------------|
| Rust | stable（MSRV **1.99**） | edition 2021 | `rust-toolchain.toml` 统一 stable 通道并带 `rustfmt` / `clippy`；`Cargo.toml` 的 `rust-version = "1.99"` 声明最低支持版本；CI（quality / release）用 `dtolnay/rust-toolchain@master` 固定 `1.99.1` 保证可复现 |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std` | ⚠️ **已验证：strict 模式（无 compat-1-2）编译通过**。版本特有 API 限制见下方「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency | `slint_build::compile_with_config("ui/app.slint", config)` + `with_bundled_translations("translations")` |
| Slint Material Components | **1.1.0** | 项目内 `material-1.1.0/`（MIT） | 通过 build.rs `with_library_paths` 配置 `@material`。**实际只用到 ScrollView / ListView**（drawer 日志 + 详情面板）+ **MaterialPalette 色板**；**未使用 Modal / Dialog 内置组件**——项目自己的 DialogShell 是手搓 `inherits Rectangle`（坑条见 Slint 限制表）。色板经 `ui/material-theme.slint` 映射到项目设计令牌 |
| bollard | **0.21** | — | ⚠️ 版本特有类型坑见 [docs/arch/docker-bollard.md](docs/arch/docker-bollard.md)（唯一权威源） |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features = false） | `Runtime::handle()` clone 后才能 spawn；**不**用 `current_thread` |
| futures | **0.3** | — | — |
| futures-util | **0.3** | — | 提供 `StreamExt`（`.next().await`），需显式引 |
| serde_json | **1.x** | — | 用于 bollard 内部序列化 / GitHub release JSON 解析 |
| anyhow | **1.x** | — | — |
| sha2 | **0.10** | — | 自动更新包 SHA-256 校验；HTTP 走系统 `curl`，**不引入 TLS crate** |
| 渲染 | — | 纯软件渲染 | 无 Vulkan/GL/WGPU 依赖，deb 包 **不依赖 libgl1** |
| 构建优化 | — | release profile | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip |

### Slint 1.18 已验证的限制

> 这些是 Slint 1.18.1 版本已验证**仍然存在**的 API 缺失 / 行为限制 / 严格语法规则。
> 写 Slint 代码时必须遵守，不能假设旧版本兼容。升级 Slint 后重新核对本节。

#### 已知 API 缺失（版本间未修复）

| 限制 | 实际行为 | 必须的替代方案 |
|------|----------|----------------|
| **TabWidget 没有 `current-tab` property** | 无法通过编程式方法切换 Tab | 只能手动点 Tab；如需自动切，改成条件渲染 `if root.show_logs { LogPanel { ... } }` 替代（本项目 `drawer.slint` 已用自绘 TabBar） |
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
| **M3 ScrollBar 滑块自动隐藏，采样必须处于悬停态** | 滑块 `opacity` = 滚动中 40% / 悬停 70% / 静止 1.5s 后淡出；鼠标移开后截图必然采样不到滑块（只剩轨道）。验收时先真实悬停到滚动条上再 `agkit shot`，或调试时读 MCP 元素属性核对 `opacity` 值 |

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
| `ui/` | Slint 声明层（41 文件按职责拆分）：app（MainWindow 唯一组装入口）/ app-actions（UI→Rust 回调 global）/ theme（设计令牌，三态主题 auto/light/dark）/ **material-theme（M3 色板映射：Theme.* → MaterialScheme，49 个角色全覆盖）**/ models / top-bar（含更新徽标 + 设置入口）/ sub-bar / **ui-state（显示态 global：连接/采样状态 + 计数汇总 + 行模型 + 资源列表排序，Rust 单写入方）**/ **main-area（主列表工作区：SubBar + BatchBar + ListArea）**/ **list-area（资源列表区：容器视图走台账，镜像/网络/卷走表格列表）**/ **ledger（容器台账区：列宽统一计算 + 表头 + 虚拟化行）**/ **ledger-row（30px 台账行 delegate）**/ **ledger-tree-row（28px 分组带 delegate：项目带 / 服务带，通栏 + 组内资源汇总 + 组操作按钮）**/ **ledger-band（概览带：4 计数 + CPU TOP4）**/ **ledger-cols（列宽契约 global，表头与行共用）**/ **{image,network,volume}-list（三个资源表格列表：`inherits TableShell`，只配列标签/列宽 + 30px 行 delegate）**/ **components-list（列表共用基件：ListCols 列宽契约 + ListHeadCell 表头单元格 + TableShell 表格外壳）**/ skel（骨架屏占位）/ foot-bar / batch-bar / drawer（`log_only` 模式承载分组合并日志）+ drawer-detail-row / **drawer-log-row（日志行 delegate，虚拟化 ListView 的一行 = 一条日志）** / dialogs（PullDialog/CleanupDialog，继承 DialogShell 基类）/ **update-dialog（自动更新弹窗：版本对比 / 说明 / 进度 / 重启）+ update-state（更新 UI 状态 global）** / **settings-dialog（设置弹窗：外观三选 + 自动更新三开关）+ settings-state（设置弹窗开关 global）** / menu-layers（浮动菜单层）/ **overlays（窗口级浮层：排序/卡片/分组菜单 + 拉取/清理/更新/设置弹窗 + TipLayer，声明在内容之后 → 最上层）**/ **components-dialog（DialogShell 基类，手搓 inherits Rectangle，坑条见 Slint 限制表）** / **components（原子组件库，9 个按职能拆分：interact 交互 / nav 导航 / form 表单 / chip 标签 / align 对齐 / state 状态 / empty 空态 / feedback 反馈 / dialog 弹窗基类）** |
| `src/` | Rust 逻辑层：main（组装：CLI 参数 → 单实例锁 → i18n → AppState → MainWindow → ui.run()）/ **cli（命令行参数解析纯函数：`--version` / `-V` / `--help`，未知参数照常启动 GUI；`--version` 不初始化 Slint / 不连 Docker）** / **single_instance（单实例锁：Unix socket 互斥 + raise IPC，陈旧锁 connect 失败即自愈清理，Drop 释放）** / **signature（更新清单 Ed25519 验签；公钥内置，安装前在应用进程内完成）** / slint_gen（include_modules 唯一入口）/ i18n（含 `trfp` 复数翻译）/ config（持久化）/ consumer（分批 drain + UI 节流三档：不含日志 8ms / 日志混批 120ms / 纯日志 250ms，窗口判定在 `consumer/throttle.rs`）/ **event_watch（Docker `/events` 事件订阅：列表即时性的唯一来源——噪声黑名单过滤 + 400ms 合并 + 断流退避重连 + **连接建立即补一次全量刷新**（`ping` 判定 false→true 跃迁，含首次成功，见规范 #32）；空闲时零请求）** / bootstrap（stats 采样（6s 一轮，整批投递）/磁盘占用（`/system/df`：容器可写层 + 卷）/首屏（含独立的守护进程版本查询）/主题监听）/ **stale（采样过期判定 `StaleTracker`：连续 N 轮无样本才判过期，只在跃迁时上报，见规范 #36）** / models（含 `ResourceSort` 资源列表排序状态 + `models/update` 自动更新类型）/ ui_state（AppState + 刷新去重/超时 + `cleanup_counts` + `compose_collapsed`；日志面板状态拆到 `ui_state/log`（14 字段），自动更新拆到 `ui_state/update`）/ **ui_state/follow_state（日志跟随状态机：单容器 / 分组合并日志建流、Clear、幂等）** / **ui_state/log_sync（日志窗口裁剪 + UI 行模型同步账 `LogSyncPlan`）**/ **ui_state/config_io（AppState ↔ config.json 读写）**/ follow_runtime（日志流运行时：建流（单流 / 分组合并多流共享取消信号）/退避重连/探针判定/保活降级）/ log_status（日志流提示文案唯一定义处，4 条 i18n key：重连中 / 已恢复 / 已停止 / 洪峰抽样）/ **update/{mod,discover,manifest,github,version,markdown,install}（发现三级回退 / 清单一致性断言 / 校验和 / 安装方式判定；`install/managed.rs` = 启动器布局的版本目录落盘与 `current` 软链切换，详见 docs/arch/update-manifest.md）** / render（CachedModel 差量同步 + 日志行模型 Append/Reset）/ **ui_update（有上限 drain + Msg 聚合；`ui_update/drain.rs` = `DrainResult` + `UiFlag` 位掩码，声明宏是唯一声明源——字段清单 / `has_ui_changes` / `logs_only` / `merge` / 一致性测试全部生成；消息分片 `ui_update/{df,log,update}.rs`）** / ui_apply（抽屉/列表灌入 + `clear_log_panel` / `clear_log_status` / `status_toast`）/ batch / callbacks{,_container,_drawer,_image,_netvol,_compose,_sort,_update,_settings} / detail_fmt/{container,image,netvol}（详情格式化）/ ui_rows/{containers（台账行：筛选/排序/归一化比例）,ledger（概览带 + 台账入口）,compose_tree（台账内联树行：项目带/服务带 + 容器行）,group_totals（分组资源汇总：组内成员之和）,images,netvol（含排序）,sort（三个资源列表共用的「过滤→排序→映射」骨架）}（行构建）/ docker/{mod,containers,images,logs,log_batch（行批投递：前缀拼接 + 每批解析最后一行推游标 + **洪峰抽样泄压阀 `Sampling`**）,log_ts（时间戳游标：RFC3339Nano 解析/剥前缀/纳秒过滤/回放缓冲）,log_types,stats,netvol,image_details,container_detail,system（cleanup prune + `/version` 守护进程版本查询）,raw_http（裸 HTTP：bollard 未建模的接口）} |
| `translations/` | gettext zh_CN（编译期嵌入二进制，`build.rs` 通过 `with_bundled_translations` 打） |
| `docs/` | architecture.md（总纲：技术栈/模块地图/文件规范）+ arch/（threading · ui-interaction · list-layout · docker-bollard · update-manifest）· agents-rules.md（规则配置层）· git-flow.md（发版规范）· **style-guide.md（文档排版唯一规范）** · **configuration.md（config.json 15 字段全表）** · **troubleshooting.md（按症状排查）** · **translations.md（翻译机制与新增语言）**；根目录另有 CONTRIBUTING / SECURITY / SUPPORT / CODE_OF_CONDUCT 与 `.github/` 模板 |
| `packaging/` | debian/control（版本同步之一；`Cargo.lock` 由 `check-version.sh` 自动核对）+ **debian/copyright（DEP-5 许可清单）** + desktop 入口 + svg 图标 |
| `launcher/` | 独立 workspace 成员：deb 里的 `/usr/bin/docker-panel` 验签启动器（`main` 选候选 / `verify` 验签或 root 属主信任 / `exec` 用 `execveat` 复用校验过的 fd / `version` 版本比较）；依赖仅 base64 + ed25519-dalek + libc + sha2 |
| 根 | Cargo.toml（依赖 & release profile & `license = "MIT"` & `rust-version = "1.99"`；workspace 成员 `launcher`）/ **packaging/release-signing.pub（应用与启动器共用的公钥清单，唯一信任锚）**/ **rust-toolchain.toml（stable 通道 + rustfmt / clippy）** / **deny.toml（cargo-deny 许可证 allow-list）** / build.rs（slint_build 编译 UI + bundled translations）/ CHANGELOG.md / **LICENSE（自有 MIT）/ LICENSING.md（Slint 许可选择）/ THIRD-PARTY-NOTICES.md + licenses/（第三方通知）** / material-1.1.0（Slint Material，MIT） |

## 线程模型（三线程架构，已实现优雅退出）

```
主线程 (ui.run() 阻塞)       consumer 线程               tokio runtime (multi-thread)
┌───────────────────┐      ┌────────────────────┐      ┌──────────────────┐
│ docker /events ───┼──lock▶│ recv_timeout(≤100ms)│      │ events → refresh   │
│ 6s stats interval ┼──lock▶│ 分批 drain ≤2048/批 │      │ 6s stats join_all │
│ UI 回调 ───────────┼──lock▶│ pending.merge       │      │ 30s volume df     │
│                     │      │ 分档节流 apply      │      │ bollard API 调用   │
│                     │      │ 只重建变化区域       │      │ tx.send(Msg)       │
│                     │      │ drop lock ─────────┼─invoke▶│ follow_logs 流     │
│                     │      │   _from_event_loop  │      │                    │
└───────────────────┘      └────────────────────┘      └──────────────────┘
     Mutex<AppState>              mpsc::Receiver<Msg>（独占，不进 Mutex）
     + shutdown: AtomicBool
```

**关键设计**：Receiver 独占不进 Mutex（死锁唯一保证）· Slint Timer 已废弃（软件渲染器 + MCP 模式下 Slint Timer 不触发，改 tokio interval）· 两阶段铁律（锁内分批 drain + 只 build 变化区域 → drop lock → 锁外 `invoke_from_event_loop`，闭包绝不碰 state）· 日志洪峰防护（单批 ≤2048、UI 分档节流 8/120/250ms、纯日志不重算列表/抽屉、日志行模型只追加不重建）；stats 6s 一轮整批投递· Docker 调用 10s 超时 + 列表刷新 in-flight 去重（刷新期间来的请求只置脏标记、本轮结束补跑）。完整推导与代码示例见 [arch/threading.md](docs/arch/threading.md)。

## 消息类型（`models::Msg`）

| 变体 | 来源 | UI 行为 |
|------|------|---------|
| `Containers(Result<Vec>)` | Docker `/events` / 手动刷新 / 操作完成 | 替换容器列表（保留旧 stats） |
| `Images(Result<Vec>)` | Docker `/events` / 操作完成 | 替换镜像列表 + 后台补查层数 |
| **`Networks(Result<Vec>)`** | Docker `/events` / 手动刷新 / 操作完成 | 替换网络列表（网络视图） |
| **`Volumes(Result<Vec>)`** | Docker `/events` / 手动刷新 / 操作完成 | 替换卷列表（卷视图） |
| **`DockerVersion(Result<String>)`** | 启动时独立任务查询 `/version`（不阻塞首屏列表） | 写入 `DrainResult.docker_version`（底栏版本文案：`v{版本}`；取不到写占位符 `—`，不伪造版本号） |
| `PullProgress(String)` | pull_image 进度闭包 | 状态文本更新 |
| `PullDone(Result)` | pull_image 结束 | 关弹窗；列表交给 `image/pull` 事件（不再 `refresh_now`） |
| `ActionDone(desc, Result)` | 启停重启删除操作 | 状态文本；列表交给 `/events`（不再 `refresh_now`；合并窗口内先刷一次、窗口到期再补刷一次） |
| `CleanupDone(Result)` | cleanup prune 完成 | 状态文本 + 标记 `refresh_now`（prune 事件支持按守护进程版本而异，保留兜底） |
| `ContainerDetail(Result<ContainerDetail>)` | inspect_container | 替换 `AppState.detail` → Drawer 详情 Tab |
| **`LogBatch(u64, Vec<String>)`** | follow_logs Stream | `(stream_epoch, 行列表)`；不匹配当前流则丢弃 → 推入 `AppState.log.logs`（滚动窗口 ≤2000 行）→ 追加到 UI 日志行模型 |
| `LogStreamEnded(u64)` | 日志流结束（**单个 stream**） | 仅当前 stream_epoch 且**全部** stream 都退出（`follow_active` 无存活标志）时才清空 `follow_id` / `follow_targets` / `follow_cancel` / `follow_active`；分组合并日志有 N 条流，一条结束不代表整组结束。来源：Eof / Cancelled / 致命错误（容器已删除） |
| `LogStreamStatus(u64, String)` | follow_runtime 判定结局 / 洪峰抽样 | `(stream_epoch, 文本)`；仅当前流生效，写入日志面板工具栏状态位（重连中 / 已恢复 / 已停止 / 洪峰抽样），不占用底部状态栏。时机：「日志流中断，正在重连…」= 首次进入可恢复重连；「日志流已恢复连接」= 重连后收到首行日志 / Idle 结局 / 探针由不可达跃迁为可达（三者均要求此前提示过重连中）；「日志流已停止」= 非取消的结束；「日志洪峰：已按比例抽样显示，省略 N 行」= 通道积压超阈值期间每 2s 刷新一次，退出抽样时发空串清除（仅单流） |
| `ImageDetail(Result<ImageDetail>)` | inspect_image | 替换 `AppState.image_detail` → 镜像抽屉 |
| `ImageLayers(Vec<(String, usize)>)` | 镜像层数回填 | 更新 `image_layers` 缓存 |
| `VolumeSizes(HashMap<String, u64>)` | 30s `/system/df` | 更新卷磁盘占用列 |
| **`UpdateCheckDone(bool, bool, Result<UpdateInfo, String>)`** | `update::spawn_check`（启动 / 手动） | `(manual, auto_installing, result)`；成功写入 `AppState.update.update_info` 并决定顶栏徽标；手动检查失败显示在更新弹窗，启动检查失败静默 |
| **`UpdateProgress(i32, String)`** | 下载 / 安装任务 | 更新弹窗进度条与状态文案（按 1% 节流） |
| **`UpdateDone(Result<String, String>, bool)`** | 安装任务 | 结果文案 + 是否需要重启；成功时标记 `update_restart_needed`，顶栏徽标转为琥珀色等待重启 |
| **`StatsBatch(Vec<(String, ContainerStatsSnapshot)>)`** | 6s tokio interval + 首屏加载补采一轮 | **整轮一次投递**（同轮容器同帧更新）→ 写入 `AppState.stats_prev` → `compute_cpu_delta` → 容器行 CPU%/内存/网络列；本地无 prev 时用 Docker 同帧的 `precpu_stats` 兜底 → **首帧即真实 CPU%** |
| **`StatsStale(bool)`** | stats 循环的 `StaleTracker`（连续 3 轮无样本才跃迁） | 顶栏状态位「实时数据可能已过期」（`Theme.warn` 小圆点 + 文案，不弹窗、不打断）；恢复时自动清除（见规范 #36） |
| **`DiskStale(bool)`** | 30s `/system/df` 循环的 `StaleTracker`（连续 2 轮无样本才跃迁） | 同上；与 stats 同时过期时合并为「实时与磁盘数据可能已过期」 |

## 代码规范

1. **单文件 ≤ 300 行**——统计口径见顶部硬约束 R2（去除头部与注释后计正文）。当前所有文件均合规。
2. **Mutex 锁持有最小化**——锁内**绝不**做任何可能耗时的操作（await、invoke_from_event_loop、长时间迭代）。锁内只做 clone / 取字段 / spawn 启动。锁中毒（`PoisonError`）**禁止静默吞掉**：`let Ok(..) = .. else` 分支用 `crate::lock::poison("上下文")` 留痕，`if let` / `match` / `.unwrap_or` 站点用 `crate::lock::acquire(&state, "上下文")`。
3. **Receiver 不进 Mutex**——`mpsc::Receiver` 由 consumer 线程独占所有权，绝不能放回 `AppState`。
4. **所有 spawn 都走 state.rt**——`let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(async move { ... });`
5. **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。
6. **Slint 子文件不互相 import**——避免循环依赖和跨文件 global 兼容性坑。delegates 用 callback 属性注入替代直接 import AppActions global。
7. **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸，实例化处硬设。
8. **命名**：Rust 常规 snake_case / PascalCase；Slint 全局函数 snake_case，结构体 PascalCase。
9. **错误处理**：Docker API 错误统一转为 `Result<T, String>` 上抛，UI 层展示中文文本。
10. **可重试 Docker 调用必须走 `docker::with_timeout`**（列表 / stats / inspect / 磁盘占用）；列表刷新依赖 `refresh_inflight` 去重，禁止在 interval 里无界叠加任务。
11. **高频 Msg 不得逐条 invoke**——统一走 `consumer` 的分批 drain + 按批分档节流（不含日志 8ms / 日志混批 120ms / 纯日志 250ms）；UI 只重建 `DrainResult` 标记的变化区域（容器/镜像/网络/卷各自独立标记），日志走行模型追加。
12. **可能永久阻塞的读必须可被 shutdown 唤醒**——例如 dconf reader 放独立线程，监控线程用 `recv_timeout` 轮询；禁止在会被 join 的线程里做无超时的阻塞读。
13. **日志流重连必须退避且永不放弃**——Docker 流异常（`LogStreamEnd::Error`）按 `retry_delay` 指数退避重连（1s→2s→4s→8s→10s 上限），禁止立即紧密重试拖垮 Docker；连续失败预算 `MAX_RETRY_ATTEMPTS = 8` 耗尽后转 30s 低频保活（`KEEPALIVE_INTERVAL`），**不得静默放弃跟随**。超过 `LOG_IDLE_TIMEOUT`（180s）无任何日志视为连接挂死，走 `KeepAlive` 分支：静默重连、**不计入失败预算**、不打扰用户。容器是否还活着一律由探针 `probe_container` 判定（`Running`/`Stopped`/`Gone`/`Unreachable`），禁止用跨流共享的 running 标志。每条流的存活标志必须独立，旧流退出不得误标新流。
14. **日志流状态提示（四态）**——文案唯一定义处是 `src/log_status.rs`（禁止内联字面量）：「日志流中断，正在重连…」/「日志流已恢复连接」/「日志流已停止」/「日志洪峰：已按比例抽样显示，省略 N 行」。「已恢复连接」只允许三个触发点：重连后收到首行日志、Idle 结局、探针由「不可达」跃迁为「可达」；三者均要求此前提示过「正在重连」。洪峰提示的触发条件见 #33。切换资源或 Clear 必须清掉残留提示（`ui_apply::clear_log_panel` / `clear_log_status`）。
15. **日志行模型只追加不重建**——UI 日志行模型（`render::logs_sync`）是日志窗口（`AppState.log.logs`）的**镜像**，用「全局行流位置」对齐：起点相同 → `LogSyncPlan::Append`（只 `extend`，可见行不重排 → 不卡）；起点不同（窗口裁剪过前段 / 换了流）→ `LogSyncPlan::Reset`（整表替换）。**禁止**逐行丢最旧行（整表索引每行错位一次，ListView 每次都把可见行标脏重排 = 旧卡顿主因）；裁剪必须分块（`LOG_ROWS_MAX = 2000` → 保留 `LOG_ROWS_KEEP = 1000`）。同步账实现见 `src/ui_state/log_sync.rs`。
16. **状态栏 toast 必须自动恢复**——回调直接写状态栏（复制日志 / 清空日志 / 复制字段值）一律走 `ui_apply::status_toast`，2s 后恢复「就绪」；期间若被其它消息改过则不覆盖（避免旧定时器盖掉新结果）。
17. **Wayland 必须显式设置 xdg app_id**——`main.rs` 在 `MainWindow::new()` 之后、`ui.run()` 之前调用 `slint::set_xdg_app_id("docker-panel")`（仅当 `WAYLAND_DISPLAY` 存在时）。不设置时 toplevel 没有 app_id（Wayland trace 里查不到 `xdg_toplevel.set_app_id`），GNOME Shell 无法把窗口匹配到 `docker-panel.desktop`，**面板坞（Dock）会显示成「未识别程序」、图标退化成通用占位图**。仅在 Wayland 下设置：非 Wayland 会话由 winit 用 argv[0] 回退生成窗口类名（class 段 = `docker-panel`，与 `.desktop` 的 `StartupWMClass` 一致），显式设置反而会让 class 段变空（Slint 固定传 `with_name(app_id, "")`）。
18. **UI 状态单一写入方——回调写，consumer 禁回写**——用户操作过的 Slint 属性（`sort_field` / `sort_desc` / `filter` / `view`）一律由 UI 回调独占写入：回调在 UI 线程里同步改 `AppState` → 立刻 `ui.set_xxx(...)` → 重建受影响的列表。consumer **只刷数据、不回写这些属性**：它在锁内取快照、锁外才写 UI，期间用户改了条件时回写的就是**旧值**（排序状态与上下箭头偶发对不上）。需要防「锁内建行、锁外应用，期间条件被改」的竞态时用 `AppState::rows_epoch`（per-list 位掩码）：排序/筛选/搜索变更时按受影响列表递增（搜索同时作废容器 + 镜像，网络 / 卷不受影响），consumer 应用行模型前逐列表比对，该列表代际变了才丢弃这批旧行（回调已用新条件重建过）。
19. **TouchArea 命中顺序——热区声明在布局之前**——Slint 按声明顺序做命中测试，后声明的兄弟节点先拿事件。按钮的「整块热区」TouchArea 若声明在 HorizontalLayout **之后**，会盖住布局内部自己声明的 TouchArea（死代码：点箭头只会触发外层热区）。统一写法：`StateArea` / TouchArea 在前、内容布局在后（见 `Chip` / `ViewTab` / 排序按钮）；布局里的 `Text` 无 input region，点击会穿透到下面的热区。
20. **台账列宽必须由父级统一计算后显式下发**——Slint `HorizontalLayout` 先给每个子项「首选宽度」（`Text` = 文本自然宽度），再把剩余空间按 `horizontal-stretch` 分。弹性列（名称 / 镜像）若直接写 `horizontal-stretch: 1`，文本一长列就被撑宽 → CPU / 内存列起点**逐行漂移**，表格失去扫读价值。正确写法：`LedgerArea` 算出 `name_w` / `image_w` 后显式下发给表头与行（`width: root.name_w; horizontal-stretch: 0;`）。响应式隐列（磁盘 / 端口）也由父级统一判定，表头与行共用同一组判定。调试时可读 MCP 元素树逐列比对 `absolutePosition.x` 与 `size.width`，屏幕验收仍以 `agkit shot` 出图为准。详见 [docs/arch/ui-interaction.md](docs/arch/ui-interaction.md) §3.3。
21. **排序基线一律自然升序**——`sort_containers` 各分支的比较器都必须是「小的在前」，`sort_desc` 只负责反向。曾经 CPU / 内存 / 磁盘三列的基线写成降序，导致 `sort_desc=true`（UI 上的 ↓ 箭头）反而从小到大排，与「名称」列和箭头方向全都不一致。
22. **compose 分组是分组，不是筛选器**——台账内联树（`ui_rows/compose_tree.rs` 产出的分组带 + 容器行）没有「范围筛选」概念：排序字段只决定**组内**顺序，组间按组名稳定排。`compose_collapsed`（**收起的**分组键集合）由 `callbacks_compose.rs` 独占写，consumer 只读；不在集合里 = 展开 → 新分组默认展开。概览带与筛选 chips 的计数一律取**可见容器集合**（实际渲染成行的那些容器），折叠分组后两个数字同步变小。
23. **分组带的命中顺序与几何**——分组带**通栏**（独立 delegate `kind=1/2`，不参与列宽计算），内部镜像 `ledger-row.slint` 的列结构（同一套 `LedgerCols` + 父级下发的同一组 `name_w` / `image_w` / `show_disk` / `show_ports`），使带内数值与容器行**同 x**；展开箭头落在状态点列内（按 `depth*18` 缩进），分组名热区从名称起点开始 → 两者**不重叠**，互不抢事件（规范 #19）；分组名落在 `44px + depth*18`（箭头 16px + 色点 18px + 间隙），与**同层**容器行的名称列起点逐像素对齐，层级只靠缩进表达；无 Compose 归属的容器直接排在顶层（depth=0），不另设「独立容器」分组带。
24. **资源列表的排序状态三个视图各自独立**——`ResourceSort { field, desc }`（列下标 + 方向）在 Rust → Slint 之间**打包成一个 int**（低 3 位=列下标，第 3 位=降序标记）；Slint 1.18 不支持 `%` 与 `&` 运算符，解包**统一走** `ui/list-area.slint` 的 `SortCodec`（`field_of` = `Math.mod(v, 8)`、`desc_of` = `v >= 8`），与 Rust 侧 `ResourceSort::SORT_FIELD_MASK` / `SORT_DESC_BIT` 注释互指，且越界值在 `from_config` 回落默认。排序状态只由 `callbacks_sort.rs` 写（规范 #18），consumer 只读。表头方向箭头：左对齐列跟在标签后（不占布局宽度），右对齐列用**覆盖层**放在列左端（值仍右对齐，列起点不漂移）。
25. **分组带的资源汇总是「成员之和」**——分组带显示该组的 CPU / 内存 / 磁盘**总和**，落在台账的 CPU / 内存 / 磁盘列上（与容器行同 x，规范 #20）。口径：CPU / 内存只累加**运行中**成员（停止的容器行上显示「—」），磁盘累加**全部**成员（`disk_used` = 镜像 + 可写层 + 卷，累计值，停止后依然成立）；一个运行成员都没有时 CPU / 内存显示「—」。汇总描述的是「这个组占了多少」→ **与组是否展开无关**，收起时数字照旧。**微条口径 = docker stats 口径**（与 `docker stats` 的两个百分比同义）：CPU = **相对单核**的百分比 / 100（1.0 = 占满一核），内存 = `mem_usage / mem_limit`（1.0 = 占满配额）。**刻意不用「视图内最大值归一」**：那种相对刻度下分组带与容器行**量纲不同**（分组是成员之和，成员是个体）→ 同一列里出现「组数字是成员的 2 倍、条却一样长」（唯一分组时组条恒 100% 满格，零信息量），成员的条还被压掉一半分辨率。绝对口径下两者同量纲（都是「占上限的比例」），且**分组带的条 = 组内成员条之和** → 父子关系自洽。**最小可见补偿** `MIN_BAR_RATIO = 0.02`（≈1px/52px 轨道）只作用于**大于 0** 的比例，且**必须在成员层做**（分组带累加补偿后的成员条）→ 否则组条会比成员条之和还短，父子反向失真；概览带 TOP4 的 240px 轨道下绝对占比本就可见，**不加补偿**（保留排名区分度）。实现见 `src/ui_rows/containers.rs`（`bar_ratio` / `mem_ratio`）与 `src/ui_rows/group_totals.rs`（`GroupTotals::cpu_ratio` / `mem_ratio`）。⚠️ 改 `ui/ledger-row.slint` 的列结构时必须同步改 `ui/ledger-tree-row.slint`，否则带与行的数值列错位。两行**不抽共享组件**（已评估：同构的只有中段三个指标列，它们已共用 `ledger-cols.slint`；抽出需转发 5 个属性并嵌套布局，spacing 误差会直接破坏列起点契约，理由见 `ledger-tree-row.slint` 文件头）。
26. **分组带 = 组级操作入口，按钮与容器行同构**——分组带的 3 个按钮（主操作 / 日志 / 更多）与容器行**一一对应**，但动作**统一作用于组内全部成员**：主按钮按「是否全部运行中」在启动全组 / 重启全组间切换（启动只动没跑的、重启只动运行中的）；日志打开**合并日志**（每个运行中成员一条流，行首 `[容器名]` 前缀，抽屉走 `log_only` 模式隐藏详情/环境 Tab；无运行成员时只提示不开抽屉）；「更多」菜单 = 停止全部 / 重启全部 / 合并日志 / 删除全部（`CardMenu.kind = 4`）。成员口径 = `ui_rows::group_members`（**当前筛选后**的容器，与台账可见行同源）→ 筛选外的容器不会被组操作误伤。合并日志的实现约束：`follow_targets` 保存 `(id, 前缀)`、多流共享同一个 `watch` 取消信号与 `stream_epoch`、`LogStreamEnded` 只在全部流退出后清状态、`clear_log_buffer` 重建**全部**成员流；多流时**不发**重连/恢复/停止提示（单路结局代表不了整组）。`SelKind::Group` 下迟到的容器 inspect 结果必须丢弃（防止覆盖组标题/Tab 模式）。
27. **底部状态栏的资源汇总由 consumer 独占写入**——`app.slint` 的 `cpu_total` / `mem_total` / `disk_total` 三个属性绑定到 `FootBar`，**必须**在 `consumer::apply_ui` 里每轮 `set`（Rust 侧不写 → 恒显示 "0%" / "0MB"）。口径 = **全部容器**（与「N/N 容器」同源，不是筛选后的可见集合）：CPU / 内存只累加**运行中**容器（停止容器的 `cpu_percent` 是停止前的残留值），磁盘 = 镜像（按 id 去重）+ 可写层 + 挂载卷（按卷名去重），口径见规范 #28——与分组带汇总（规范 #25）同口径。
28. **排序键必须与该列的显示值同源**——台账「磁盘」列显示 `disk_used`（镜像 + 可写层 + 挂载卷），排序就必须用 `disk_used`；`size_rw` 只保留「可写层」本义，别拿它当显示值。该字段**不来自列表接口**（`size=1` 要为每个容器遍历文件系统：24 容器 70ms vs 32ms，且删除中的容器会命中 dockerd `404 rw layer snapshot not found` 让整批刷新失败）→ 由 30s 一次的 `/system/df?type=container` 回填进 `AppState.container_sizes`，列表刷新后用 `AppState::apply_container_sizes` 补回（新容器等下一轮采样，期间显示 0）。**口径 = 镜像 + 可写层 + 挂载具名卷**（镜像按 `image_id` 查 `images[].size`，卷查 `volume_sizes`）：只算可写层会得到 8K / 4K 这种像坏了的数字，漏掉镜像又会低估 20–100MB 量级。**行 = 这个容器占了多少**（共享镜像各计一遍，与 `docker ps -s` 的 `virtual` 同口径）；**底栏合计必须去重** = 镜像按 id + 可写层求和 + 卷按卷名。推导见 [arch/list-layout.md](docs/arch/list-layout.md) §容器磁盘列。卷与容器两路采样互相独立，任一路到达都要重算（`Msg::VolumeSizes` / `Msg::ContainerSizes` / `Msg::Containers` 三处都调 `apply_container_sizes`）。曾经用 `block_read + block_write` 排序，而这两个字段从未被赋值（恒 0）→ 所有容器排序键相等 → 稳定排序保持原序 → **点「磁盘」表头毫无反应**（而它恰好是默认排序字段）。字段恒 0 又无写入方的，直接删掉，别留着当陷阱。
29. **自动更新必须走「签名清单 → 验签 → 原子安装/自替换」，并 spawn_blocking**——GitHub 检查 / 下载 / `pkexec` / 文件 IO 全是阻塞操作，统一放 `tokio::task::spawn_blocking`，禁止占用 tokio worker 或 UI 线程；HTTP 只走系统 `curl`（参数数组直传，禁止拼 shell）；下载后先核对 `checksums.txt` 的 SHA-256，再取**已签名清单**（`version.json` + `version.json.sig`，用 `packaging/release-signing.pub` 的**内置公钥清单**逐个验签——清单支持轮换、坏条目跳过，验不过即拒绝，fail closed；`checksums.txt.sig` 同钥签名，手工下载也能离线复核）。**签名私钥绝不进 CI**：流水线只建草稿 Release（8 个构建产物）并校验齐全，随后由维护者本机 `scripts/sign-release.sh <version>` 离线签名 `version.json` 与 `checksums.txt`、补传 `.sig`、才 `draft=false` 发布并同步 master；CI 被攻破最多改草稿，签不出能过校验的清单（见 `docs/git-flow.md` §3）。**静默更新有两种安装形态，都不需要 root**：①用户自己装的二进制 → `install_binary` 同目录 rename（旧文件留 `<exe>.previous`，只保留最新版本）；②deb 安装 → `/usr/bin/docker-panel` 是**验签启动器**，运行体在 `/usr/lib/docker-panel/seed/`，更新落到 `~/.local/opt/docker-panel/versions/<v>/`（运行体 + `version` + `checksums.txt` + `.sig`，先 staging 再整目录 rename，最后原子替换 `current` 软链），由启动器重启——启动器在种子与用户副本间挑「版本更高且校验通过」的那份 `execveat`（用户副本必须验签；种子按 root 属主信任，因为签名在 deb 构建后才产生）。目录不可写且非种子布局才退回 `pkexec dpkg -i`（每次授权，属权限边界）。`already_installed` = dpkg 版本 ≥ 清单版本**且比当前运行的版本新**（重启之后不再残留「重启后生效」的假状态）；此时只提示「重启生效」，禁止重复下载；自动检查节流 **1h**（轮询 30min），已下载且 SHA-256 对得上的包直接复用，`auto_restart` 开关必须真正控制是否重启。开发构建与非 Linux 只提示不安装；检查与安装共用 `update_busy` 互斥，进度按 1% 节流投递 `Msg`。
30. **Docker 客户端必须共享单例（OnceLock + clone），禁止调用点自建**——bollard `Docker` 是 `Arc<Transport>`（hyper 连接池），clone 极廉价且池共享；`docker::connect()` 已做成进程级单例。此前「每次调用新建」让每 3s 刷新建 4 个客户端、每 6s stats 每容器再建 1 个（11 容器 ≈ 每分钟 110 个），用完即弃、连接池零复用，每次请求都重走 Unix socket 建连。新增调用点一律 `crate::docker::connect()`；`stats_prev` 必须随容器删除回收（`Msg::Containers` 应用列表时按存活集合 retain），StatsBatch 应用先建 id→下标映射（禁止逐条 `iter().find` 的 O(批量×容器数)）。
31. **断流补齐的游标纪律——json-file 全流带时间戳、每批只解析最后一行、每流独立、首发/Clear 不用 since**——日志流因 `Eof` / `Error` / `Idle` 断裂后，重连可带 `timestamps + since` 补回断流窗口内的行（`docker/log_ts.rs` 解析 RFC3339Nano、剥前缀、纳秒过滤、回放缓冲）。口径：①**驱动探测在建流时做一次**，`json-file` → 该流**所有**连接（含首次建流、含分组合并的每一条流）一律 `timestamps(true)`，用于建立游标；非 `json-file`（journald / none / 探测失败）→ 退回旧行为（不传 timestamps、不传 since、tail 原值）。②**首次建流与 `Clear` 永远不带 `since`**（否则历史复活 / 清空失效），只有断裂后的重连路径才带 `since(游标秒)` + 纳秒级 `ts > cursor` 过滤。③**成本优化**：正常段每行只切首个空格剥前缀（**不解析**），每批投递只对最后一行做一次完整 RFC3339Nano 解析来推进游标；只有回放段才逐行解析过滤。④**游标必须每流独立**（分组合并日志的 N 条流各记各的，**禁止**放进 `AppState` 全局，否则多流互相覆盖）。⑤安全阀：连续 2 次重连收到的行全被游标过滤且 `attempt ≥ 2` → 丢弃游标退回 `tail=0`（仍带时间戳重建），只 `eprintln` 不占状态栏。⑥bollard 0.21 的 `since` 是 Unix 秒 **i32**（`LogCursor::since_secs()` 用 `clamp(0, i32::MAX as i64) as i32` 防溢出）。单次回放上限 `REPLAY_MAX = 2000`（与日志窗口上限对齐，只保留最新一段），不破坏规范 #15 的只追加/分块裁剪与 #26 的多流独立账本；剥前缀顺序固定为「切首个空格 → trim → 加 `[容器名]`」，面板与「复制日志」逐字与修复前一致。
32. **列表即时性靠事件，没有轮询兜底**——`event_watch::spawn` 常驻消费 Docker `/events`（只筛 container/image/network/volume），收到非噪声事件就 `refresh_kinds(kinds_of(&ev))`——**按类型只刷那一路**（容器事件只拉 `/containers/json`，实测 18ms vs 全量 77.7ms；镜像引用数本地重算）；400ms 合并窗口是**双沿**的：首事件立即 `refresh()`（反馈要快），窗口内后续事件只记账、到期由 `select!` 定时器补刷一次（`Debouncer`）——**只丢不补会让窗口尾部的 `stop` 被吞到下一个事件**，列表与真实状态不一致；代价上界是每窗口多一轮（实测 ~70ms 墙钟、应用 CPU 0–10ms）。断流按 1s→30s 退避重连且永不放弃。**禁止**再加「定时兜底轮询」：3s 轮询每分钟白烧 dockerd ≈3.3s CPU，而它只会让 `/events` 的漏刷、过滤写错、代理不支持长连接这些**真 bug** 表现成「最多慢 15s」，把根因藏起来；现在断流会 `eprintln!("[events] stream error")` 直接现形。动作过滤用**黑名单**（只忽略 `health_status` / `exec_*` / `attach` / `detach`）而不是白名单——未知动作宁可多刷一次也不漏刷。`refresh()` in-flight 期间只置 `refresh_dirty` → 本轮结束补跑（否则并列合并会吞掉刷新）。**连接建立时必须补一次全量刷新**：建流前 `ping` 判定，`ConnState` 只在 `false → true` 跃迁时刷新（首连 / 重连成功 / 连续失败后成功各一次）——断流窗口的事件永久丢失，**dockerd 后起时列表要自己恢复**；单次失败与重连中的多轮失败不得重复刷新。**操作消息不再自带刷新**：启停/重启/删除、镜像删/拉、网络卷增删都实测有 `/events`，`ActionDone` / `PullDone` 只写状态文本；只有 prune（`CleanupDone`）保留 `refresh_now` 兜底。别再给操作消息加刷新，否则每次多付一轮 4 个 list 请求。
33. **日志洪峰必须按通道积压抽样，且不破坏游标**——通道是无界队列，UI 侧跟不上时积压会无限涨（RSS 与显示延迟双失控）。生产者（`docker/logs.rs`）每批投递 `+n`、consumer 应用时 `-n`（`AppState::log_backlog`，递减点唯一：`AppState::drain_backlog`），积压 / `LOG_BACKLOG_HIGH`（2 万行 ≈ 2s 满额消费量）= 抽样步长，封顶 `LOG_SAMPLE_MAX`（1/64）→ **反馈式**收敛，不需要知道 consumer 的确切消费能力。抽样**只影响显示**：`last_token` 在抽样判定**之前**更新，游标仍指向本流最后一行（#31 的补读不会退回去重放）；回放段不抽样（本来就有 `REPLAY_MAX` 上界）。丢行必须可解释：工具条文案走 `log_status::NOTICE_SAMPLED`（每 2s 刷新，退出抽样发空串清除，**仅单流**——多流共享 epoch 会互相覆盖），并用 `eprintln!("[logs] flood sampling …")` 留痕。⚠️ 实测：7.6 万行/s ≈ 0.34~0.36 核、127 万行/s ≈ 1.3~1.5 核，两种码率下积压**从未触及 2 万行阈值**（consumer 每行约 40ns ≪ 解码约 1µs）→ 该阀在本机不触发，仅作安全阀，别拿它当 CPU 优化点。
34. **空闲不重绘由事件驱动结构保证，禁止再加逐字段比对**——列表只在 `/events` 非噪声事件（#32）、stats 6s、卷占用 30s、首屏/更新调度/主题监听这些**真实变化源**到达时才 apply/重绘，没有「定时心跳拿同一份数据反复 apply」。**每次 apply 有约 6ms 固定重绘成本**（软件渲染器按行标脏 + 布局），所以「提周期」与「拆片投递」都是乘成本的写法：实测 6s 单批 0.19% → 2s 单批 0.52% → 2s 拆 8 片 2.3%（拆片只摊平重绘、不提高单行新鲜度）→ **保持 6s 单批**。**显示死区**：CPU% 变化 < 0.5pp、内存变化 < min(用量 2%, 配额 1%) 时不写模型（`ui_rows::{hyst_cpu,hyst_mem}`，在 `ui_update` 的 `StatsBatch` 应用）——行不变就不标脏；交替 A/B（60s UI 线程 schedstat）旧 0.206% / 0.197% → 新 0.086% / 0.076%。⚠️ 旧值为 0 时必须直接落值，否则「从 0 起步」的小容器会被死区钉死在 `0B` / `0.0%`（踩过）。阈值细节见 `docs/arch/threading.md` §5「显示死区」。已决定**不实现**逐字段比对兜底（加 `PartialEq` 且排除 stats，收益≈0、维护负担大），真出现无谓重绘先查变化源。
35. **单实例锁必须在 CLI 分支之后、任何 Slint / Docker 初始化之前**——`main.rs` 顺序固定为 `cli::parse_args` → `single_instance::acquire()` → `i18n::init()` → `AppState::new()` → `MainWindow::new()`：`--version` / `--help` / `--status` 必须先返回，不能被锁挡住；`AlreadyRunning` 时提示后 `exit(1)`（脚本可据此判断），不初始化 Slint、不连 Docker。**互斥是两道锁，权威的那道不落盘**：① 抽象命名空间 socket——`@docker-panel-lock-<uid>` **按用户全局唯一、与任何文件路径无关**（TMPDIR / `XDG_RUNTIME_DIR` / 运行目录怎么变都撞同一个名字），另加路径派生名 `@docker-panel-<16hex>` 与 ≤0.8.1 的老版本互斥；没有文件、外部删不掉、进程退出内核回收。② 文件 socket `/run/user/<uid>/docker-panel-<uid>.sock`（不可用退 `/tmp`）——**只**承担 raise IPC 与可见性，拿不到也不影响互斥，绝不因它拿不到就放行第二个实例。锁路径**按 uid 派生、绝不读 `XDG_RUNTIME_DIR`**（桌面 / 终端 / systemd 下该变量可能不同或缺失 → 两套路径互不冲突 → 照样多开；本机曾因基准脚本每轮 `rm -f` 掉 socket 真出过 8 个窗口）。抽象 listener **绝不能移交 raise 线程**（guard drop 后线程仍持 fd → 抽象名不释放 → `release_then` 重启时新进程判已运行后退出、应用消失）；`--status` 用「尝试 bind 抽象锁」判定，**不** connect 它（不放行 accept 会留一条永不释放的连接）。陈旧 socket（SIGKILL / `panic=abort` 残留）启动时 `connect` 失败即清理重绑；升级过渡期对旧命名 `docker-panel-<user>.sock` 做活体探测；`InstanceGuard::drop` 删除文件 socket。推导与坑条见 `src/single_instance.rs` 文件头。窗口前置只做 `show()` + `set_minimized(false)` + `request_redraw()`，**是否真正置顶由 compositor / WM 决定**：**GNOME Wayland 下做不到**——token 由焦点客户端创建并转交，终端启动的第二个进程没有 `XDG_ACTIVATION_TOKEN`；winit 的 `focus_window()` 在 Wayland 是空函数、`set_minimized()` 被忽略，`request_user_attention` 与 GTK `present()` 实测均无效，桌面文件无 `DBusActivatable`（本项目不为其它显示后端单独加 feature 或代码路径）；禁止为此引入 xdotool / zbus / 原生 wayland-client 等重型或脆弱手段（能力边界与未来方案见 `src/single_instance.rs` 文件头）；抽象锁绑定出错（非 AddrInUse，例如 fd 耗尽）→ **fail closed 退出**（`exit(1)`）：fail open 正是多开的入口。
36. **采样过期只在边沿触发上报，且不得打断用户**——stats（6s）与 `/system/df`（30s）失败是常态（单轮超时 / 容器正在删除），界面不能在单次失败时闪烁：`src/stale.rs` 的 `StaleTracker` 只在**连续 N 轮无样本**后跃迁，恢复一轮即清零计数。阈值依据「几轮之后用户已经看到旧数据」：stats `STATS_STALE_ROUNDS = 3`（6s × 3 = 18s，一次抖动不亮、20s 内可见）；`/system/df` 要遍历卷与可写层、单轮超时更常见，`DISK_STALE_ROUNDS = 2`（≈60s）。上报**只在跃迁时发一次** `Msg::StatsStale` / `Msg::DiskStale`（同值不重发，避免每个周期都触发 UI 应用），UI 落到顶栏连接状态旁的小圆点 + 一行 11px 文案（i18n key 在 `translations/`，禁止硬编码中文），**不得用弹窗 / 打断式提示**。列表失败的可感知性走既有 `docker_ok`：容器、镜像、**网络、卷**任一路失败都写 `docker_ok = false`（顶栏「未连接」），成功再翻真。

## 构建命令

```bash
cargo run                 # 开发调试
cargo build --release     # 发布构建（体积最小化 profile）

# 截图验收：agkit shot --app docker-panel（见顶部「硬约束：UI 验证统一走 agkit 截图」）

# 代码检查
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
```

> 工具链：`rust-toolchain.toml` 统一 stable 通道（含 `rustfmt` / `clippy`）；`Cargo.toml` 的 `rust-version = "1.99"` 是最低支持版本；CI 固定 `1.99.1`（quality 流水线跑 fmt / clippy / test，release 流水线用同一版本构建）。

## 发版流程

严格走 Git Flow，详见 [docs/git-flow.md](docs/git-flow.md)。四个位置必须同步版本号：

1. `Cargo.toml` 的 `version`
2. `packaging/debian/control` 的 `Version`
3. `CHANGELOG.md` 新增版本段落（`[Unreleased]` 改写为 `## [X.Y.Z]` + 底部链接；**hotfix 同样要写**）
4. `Cargo.lock` 里 `name = "docker-panel"` 的 `version`（跑一次 `cargo build` 自动写；漏了 CI 的 `--locked` 直接失败）

**发版是一条流水线，默认一次做完，不需要每次问用户**（2026-11-16 用户要求：「提交，推送，发版，更新，这个流程不要我每次来提醒」）：

1. 精确 `git add` → 按逻辑分组提交 → 推 `develop` 到**两个平台**；
2. 要发版时四处版本号同步（含 Cargo.lock） → `chore(release): X.Y.Z` → 合入 `master` 打 tag → 分别 `git push <p> master` 与 `git push <p> refs/tags/X.Y.Z`（tag 单独推，多 ref 一次推会吞掉 tag 事件）；
3. 等 GitHub Action 走完 draft → 上传 → 校验 → 发布，再 `scripts/sync-gitee-release.py X.Y.Z` 补 Gitee（无 CI，分支/tag/Release 都要手工同步；两平台并列，不是主从）；
4. 动过 `assets/screenshots/` 就逐张 purge jsDelivr，否则 Wiki 继续返回旧图；
5. 收尾 `scripts/check-release-guards.sh` + `scripts/check-version.sh`，把两平台状态与下载链接回报用户。

**发布产物不可变**：immutable releases 下已发布 Release 的资产不可增删改、tag 不可再生。流水线必须保持「draft → 上传 8 个资产 → 校验齐全 → `draft=false`」，**不要改回一步到位的 `draft: false` + `files:`**；发了错只能发 patch 版本，**禁止删已发布 Release 重发**（会让 tag 名永久不可重建）。处置表见 [docs/git-flow.md](docs/git-flow.md)。

**除顶部「破坏性操作」四类外，其余自行完成。**

## 文档同步规范

> 已迁移至规则配置层 [docs/agents-rules.md](docs/agents-rules.md) §6（触发事件 → 文档清单与职责 → Wiki 流程 → 双份同步原则）。代码变更后按该节执行同步。
> 提交前快速校验：`grep -ri dvkit .`（零残留）+ 核对 README 项目结构树 + 核对各文件行数（R2 口径）。

## 当前已知限制

- **无 Docker Compose 导入导出、无定时清理**（属 [docs/roadmap.md](docs/roadmap.md) 的候选方向，未排期）
- **镜像 / 网络 / 卷行不支持多选批量**（当前只有容器视图支持 CheckBox 多选）
- **断流窗口的日志会补回，单次最多 2000 行**（`REPLAY_MAX`）：仅 `json-file` 驱动生效；时钟回拨安全阀见 #31⑤；首次建流与 `Clear` 永不带 `since`
- **日志窗口最多保留 2000 行**：超出后分块丢掉最旧的一段（保留 1000 行）；「复制日志」复制的是窗口内容（与面板所见一致），不是容器的全部历史日志
- **容器停止后不自动重连**：探针判定容器已停止/已删除即结束流并提示「日志流已停止」，需手动切 Tab 才会重新建流
- **静默容器的「已恢复连接」提示最迟 180s**：容器 180s 无输出走 Idle 保活分支，不计失败预算也不打扰用户；「已恢复」要等到 Idle 结局才提示
- **自动更新仅支持 Linux**：装在用户目录（`~/.local/bin` 等可写位置）时由 App 静默自替换、无需任何授权；系统级 deb 安装（`/usr/bin`）仍走 `pkexec dpkg -i`，每次升级一次授权；开发构建 / 其它平台 / 无匹配资产时只提示并打开下载页
- **deb 包本身没有 dpkg/debsig 签名**（`version.json` 有 Ed25519 签名，应用内更新只装签名清单声明且 SHA-256 匹配的包；手工下载 deb 安装时仍只有 HTTPS + `checksums.txt`）。更新发现走「静态清单 `version.json` → jsDelivr → `api.github.com`」三级回退，清单只当版本号广播（资产 URL 由版本号派生），清单与 `checksums.txt` 双来源冲突即 fail closed（拒绝安装），见 [arch/update-manifest.md](docs/arch/update-manifest.md)
- **系统级 deb 升级依赖 polkit 授权代理**：无桌面 polkit 的环境只能手动下载 deb；装到用户目录则不需要

## 文档导航

- [规则配置层](docs/agents-rules.md) — 规则地图、红线约束、四阶段门禁、冲突记录、文档同步规范
- [架构设计](docs/architecture.md) — 总纲 + arch/ 主题文档（线程/消息/回调、Slint 交互、列表布局、Docker/bollard 坑条、更新清单与 CDN 分发）
- [Git Flow 规范](docs/git-flow.md)
- [文档排版规范](docs/style-guide.md) — 写或改任何文档前先读；`python3 scripts/check-docs.py` 一键自检
- [配置项说明](docs/configuration.md) · [故障排查](docs/troubleshooting.md) · [翻译贡献](docs/translations.md)
- [贡献指南](CONTRIBUTING.md) · [安全策略](SECURITY.md) · [支持渠道](SUPPORT.md)
- [路线图](docs/roadmap.md) — 当前阶段 / 候选方向（未排期）/ 明确不做
- [更新日志](CHANGELOG.md)
