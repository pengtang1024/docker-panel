# docker-panel 抽屉 UX 修复 — 产品需求文档

## Overview
- **Summary**: 修复右侧 Drawer（抽屉）三个 Tab（详情 / 日志 / 环境）的长文本折行、内容滚动、值复制三个 UX 缺陷
- **Purpose**: 用户在查看容器详情时，长 ID、长环境变量 PATH 等被截断，无法看全、无法滚动、无法选中复制
- **Target Users**: docker-panel 桌面应用的终端用户（Linux / Wayland / Slint software renderer）

## Goals
1. Drawer 详情 tab 和环境 tab 的长 value 完整显示（自动折行），不截断
2. Drawer 内容超过视口高度时 ScrollView 出现滚动条，用户可上下滚动查看全部
3. 详情 tab 和环境 tab 每一行 value 有复制入口（点击即复制到系统剪贴板）

## Non-Goals
- 不改日志 tab 的复制功能（已有 `log_copy_click` 全局按钮 + TextInput 自带选择）
- 不做 Slint TextInput 选中复制（Slint 1.18 TextInput 在深色主题下有白色边框 bug，且只读 TextInput 的选中行为不稳定）
- 不引入 Rust 侧预折行（wrap_rows 函数已验证会导致 kv 行消失，根因不明）
- 不改 Drawer 宽度（Theme.drawer_w 保持原值）

## Background & Context
- **Slint 1.18.1 + software renderer**：无 GPU 依赖，窗口尺寸 1280×760（截图证据），Drawer 宽度 ~400px
- **arboard = "3" + wayland-data-control feature 已在 Cargo.toml**：日志 tab 复制功能已用 `arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text))` 模式，可直接复用
- **当前 drawer.slint 结构**：标题栏（stretch:0）+ Tab栏（stretch:0, h=36px）+ 内容区 Rectangle（**唯一 stretch:1 的兄弟**），三个 ScrollView 各以 `height:100%` 嵌套在内容区 Rectangle 里，通过 `visible: root.tab == N` 条件切换
- **Slint 1.18 ScrollView 铁律**：多个 `vertical-stretch:1` 兄弟会平分剩余高度；只有一个最外层子元素 stretch:1 吃满，内部子元素用 `height:100%` 传递
- **当前 drawer-detail-row.slint**：三种 DetailPair kind（0=kv行 / 1=分节标题 / 2=代码盒 / 3=环境变量）的值区 VCenterText 已加 `wrap: word-wrap`（上次会话唯一有效改动）
- **上次会话教训**：引入 wrap_rows 预折行导致 kv 行消失（已回滚）；未验证滚动和复制即宣称完成；未切换到环境 tab 验证

## Functional Requirements
- **FR-1**: 详情 tab — 所有 kind=0/2 的 value（容器 ID、镜像名、CPU/内存/磁盘数值、entrypoint、command、labels 等）超过 Drawer 可用宽度时自动折行，不截断
- **FR-2**: 详情 tab — 内容总高度超过视口（标题栏+Tab+内容区）时，滚动条出现，用户可滚动查看全部
- **FR-3**: 详情 tab — 每个 kind=0/2 的 value 区有复制入口（小图标按钮或点击区域），点击后该行 value 写入系统剪贴板，底部状态栏显示"已复制 N 字符"
- **FR-4**: 环境 tab — kind=3 的每个 env 变量值折行显示，有复制入口，整体可滚动
- **FR-5**: 详情 tab 和环境 tab 的 ScrollView `as-needed` 滚动条在内容不超视口时不出现
- **FR-6**: 日志 tab 现有功能不受影响（TextInput + 复制按钮 + 清空按钮）

## Non-Functional Requirements
- **NFR-1**: Slint 严格模式无新编译错误（`cargo build --release --features slint/mcp` 零新增 warning 以外的诊断）
- **NFR-2**: 质量门禁全部通过（`cargo check --all-targets` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --all -- --check`）
- **NFR-3**: 源码零废弃旧名残留（`grep -riE 'd[v]kit' src/ ui/ Cargo.toml packaging/` 退出码 1）
- **NFR-4**: 单文件正文 ≤300 行（AGENTS.md R2）
- **NFR-5**: 不破坏 Drawer 关闭、Tab 切换、日志跟随、日志复制/清空等现有功能

## Constraints
- **Technical**: Slint 1.18.1 API 限制（TabWidget 无 current-tab 属性、TextInput 深色主题白色边框无法去除、software renderer 字体依赖系统、无 GPU）
- **Technical**: Wayland 环境（arboard 需 wayland-data-control feature，MCP 截图是唯一 UI 验证手段）
- **Technical**: ScrollView 内部 VerticalLayout 不能写 height:100%（会形成循环绑定）
- **Business**: 无预算约束，但需最小化改动范围
- **Dependencies**: bollard 0.21（Docker 数据）、arboard 3（Wayland 剪贴板）、Slint 1.18.1

## Assumptions
- 用户通过鼠标滚轮、触控板滚动、或 Slint ScrollView 自动出现的滚动条进行滚动操作
- 环境 tab 的 DetailRow kind=3 每行都是独立的 name=value 对，复制时复制完整 name=value 字符串（与日志复制语义一致）
- 复制成功与否通过底部状态栏 toast 反馈（已有 `ui.set_status_text(SharedString)` API）

## Acceptance Criteria

### AC-1: 长文本折行 (详情 tab)
- **Type**: `rule`
- **Given**: 用户打开任一容器 Drawer 详情 tab
- **When**: 某 value（容器 ID sha256:...、entrypoint、command、labels 等）文本长度超过 Drawer 内容区可用宽度
- **Then**: 该 value 自动折成多行完整显示，右侧不出现截断（...或 elide 省略）
- **Pass Condition**: MCP 截图中所有 kv 行 value 完整可见，无截断
- **Evidence**: `cargo build --release --features slint/mcp` 后 MCP take_screenshot + 视觉检查

### AC-2: 长文本折行 (环境 tab)
- **Type**: `rule`
- **Given**: 用户切到 Drawer 环境 tab，容器有环境变量且某 value 超过可用宽度
- **When**: 任一 env 变量的 value 超过 Drawer 内容区可用宽度
- **Then**: 该 value 自动折行完整显示，不截断
- **Pass Condition**: MCP 截图中 env 变量完整可见
- **Evidence**: MCP screenshot (切到 tab=2)

### AC-3: 详情 tab 可滚动
- **Type**: `rule`
- **Given**: 容器详情总行数超过视口（容器详情通常 20-30 行）
- **When**: 用户打开 Drawer 详情 tab
- **Then**: ScrollView 出现垂直滚动条（或鼠标滚轮可滚），用户能滚到底看到"配置"分节的 labels 行
- **Pass Condition**: MCP 元素树中详情 ScrollView 的内部 content 高度 > 视口高度，`dispatch_pointer_scroll` 后第二张截图内容相对位置发生可见变化
- **Evidence**: 两张对比截图 + `get_element_tree` 尺寸数据

### AC-4: 环境 tab 可滚动
- **Type**: `rule`
- **Given**: 容器有 20+ 环境变量
- **When**: 用户切到 Drawer 环境 tab
- **Then**: ScrollView 出现垂直滚动条，能滚到底
- **Pass Condition**: 同 AC-3 对比截图证据
- **Evidence**: MCP screenshot × 2 + get_element_tree

### AC-5: 复制 value 成功
- **Type**: `rule`
- **Given**: 用户在 Drawer 详情 tab 看到某行 value
- **When**: 用户点击该行 value 旁边的复制图标（或复制区域）
- **Then**: a) 该行 value 被复制到系统剪贴板；b) 底部状态栏显示 toast（如"已复制 48 字符"）
- **Pass Condition**: 点击后 Wayland 剪贴板包含正确 value（或通过 arboard 回读验证）；状态栏 toast 可见
- **Evidence**: arboard::Clipboard::get_text() 调用 + MCP screenshot

### AC-6: 现有功能无回归
- **Type**: `rule`
- **Given**: 用户执行 Drawer 关闭、Tab 切换、日志跟随、日志复制、日志清空、容器启停等操作
- **When**: 每次操作完成后
- **Then**: 行为与修复前一致（关闭=抽屉收起、Tab=切面板、日志跟随=自动滚到底等）
- **Pass Condition**: 每个操作后的截图与修复前基线截图视觉一致
- **Evidence**: 手动测试 + 截图

### AC-7: 滚动条按需显示
- **Type**: `rule`
- **Given**: 容器详情/环境总行数少于视口高度
- **When**: 用户打开 Drawer
- **Then**: 右侧 ScrollView 滚动条不出现（`vertical-scrollbar-policy: as-needed` 生效）
- **Pass Condition**: MCP 截图中无滚动条
- **Evidence**: MCP screenshot

### AC-8: 代码质量
- **Type**: `rule`
- **When**: 最终构建
- **Then**: 所有质量门禁通过（cargo check / clippy -D warnings / fmt check / 单文件 ≤300 行 / 零废弃旧名）
- **Pass Condition**: 四条门禁命令全部成功退出
- **Evidence**: shell 输出

### AC-9: 抽屉整体视觉一致性 (rubric)
- **Type**: `rubric`
- **Dimension**: 颜色 / 间距 / 对齐 / 图标风格 与现有 theme + 组件库 一致性
- **Scale**: 1-5
- **Anchors**: 1 = 明显不一致（颜色错、间距乱）; 3 = 基本一致但有些小瑕疵; 5 = 完全融合、视觉上看不出是新增元素
- **Pass Threshold**: >= 4
- **Evidence**: MCP screenshot + 对比现有 theme.slint 令牌

## Open Questions
- [ ] 用户是否需要"一键复制全部详情"（详情 tab 右上角加 copy all 按钮）？当前 spec 按每行复制实现，可后续追加
- [ ] 复制按钮放在每行的什么位置最合适？选项：(A) value 区域右边的小 iconBtn；(B) 整个 value 区域是一个 clickable 区域（hover 变 + 点击复制）。A 更明确但占空间，B 更省空间但 discoverability 稍差。**建议用 A（小 copy icon）**，参考 Drawer header 已有 icon_size:12px 的 IconBtn
