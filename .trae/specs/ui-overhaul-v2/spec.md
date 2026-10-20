# Spec: UI 彻底重构 v2

## Problem

经过第一轮 Theme 令牌系统 + 第二轮组件皮肤升级后，用户实测截图暴露了大量结构性设计缺陷。整体界面看起来"乱"、"残次品"、"现代软件设计不符"，核心问题不是皮肤而是 **布局架构**。

**用户原始反馈（逐条编号对应 AC）**：
1. 列表没有对齐 → 表 1
2. CPU/内存没有一直显示 → 表 2  
3. 分组没有树的感觉 → 表 3
4. 右侧栏全是乱的 → 表 4
5. 字体/样式/tab 都是乱的 → 表 5
6. 顶部栏也是全是乱七八糟 → 表 6
7. 底部按钮应该是 icon 按钮放上面 → 表 7
8. 容器/镜像应该是不同的按钮，tab 是什么残次品 → 表 8
9. 弹窗更奇怪，太乱了 → 表 9

## Users

docker-panel 桌面用户（Linux），已经有 Docker daemon running，需要高效管理容器/镜像/网络/卷。预期 VS Code / Podman Desktop 级别的专业工具质感。

## Goals

1. **替换 TabBar 为 Activity Bar**——左侧竖排 4 个 icon 按钮，类似 VS Code 侧栏，active 有 accent 竖条
2. **修复表头-行对齐**——确认 CPU/内存列是数据问题（Rust 侧 stats 没灌进去）而非 UI 列宽
3. **右侧详情面板卡片化分组**——每个 Section 变成一个 bg-card 卡片 + accent 左竖线标题 + 内部缩进
4. **ToolBar 改为 icon-only 按钮行**——3 个 24×24 icon 按钮排底部，hover 变色
5. **弹窗完全自定义**——PullDialog / CleanupDialog 用 Slint 组件重写（背景卡片 + 标题区 + 内容区 + footer 按钮区）
6. **Tab 去残次品**——主视图切换从 TabBar → Activity Bar；右侧内部 Tab（日志/镜像详情）保持但样式重做（accent 下划线 + 粗 active / 淡化 inactive）
7. **字体/字号统一**——Theme 令牌体系已经建立，所有 Text 必须从 Theme.text-* 取，禁止硬编码
8. **顶栏简化**——Logo + pill + Search + Refresh 四个元素，间距对齐一致

## Non-Goals

- 不新增功能（无快捷键、无 Compose 导入导出、无定时清理）
- 不改 Rust 业务逻辑层（除了 stats 数据写入 Row 结构的 bug 修复）
- 不做移动端适配（Slint 软件渲染，固定窗口 1100×680）
- 不支持缩放/动态字号

## Requirements

### FR-1: Activity Bar 替换 TabBar

主视图切换从顶部 TabBar 改为左侧竖排 Activity Bar。4 个 icon 按钮：🐳容器 / 🖼镜像 / 🌐网络 / 💾卷。active 项有 2px accent 左竖条 + accent-soft icon 颜色 + 轻微 bg-hover 背景。宽度 48px。

- **AC-1.1 (rule)**: 主布局三栏 → Activity Bar (48px) | List Area (flex) | Detail Pane (320px)
- **AC-1.2 (rule)**: Activity Bar 点击切换 active_view（0-3），Rust 侧 callback `activity_changed(int)`
- **AC-1.3 (rubric)**: Activity Bar 视觉符合专业工具侧栏质感（0-2，阈值 ≥1.5）
  - 2: icon 居中 + accent 左竖条精确贴边 + hover 过渡自然 + 宽度 48px 一致
  - 1: icon 居中但 accent 竖条位置有偏差 / hover 过渡生硬
  - 0: 布局混乱或 icon 不居中

### FR-2: 容器列表 CPU/内存数据修复

截图中容器 Tab 的 CPU/内存列显示的是停止按钮图标而非数值，根因是 Rust 侧 stats 数据（6s 采样）未灌入 ContainerRow.cpu_percent / mem_usage 字段。需要确认 ui_rows.rs 的 compose 逻辑是否正确从 stats_prev HashMap 取值。

- **AC-2.1 (rule)**: ui_rows.rs compose_container_rows 必须从 AppState.stats_prev 读取 cpu_percent + mem_usage 写入 ContainerRow
- **AC-2.2 (rule)**: stats 6s 首次到达前显示 "--" 占位而非空字符串
- **AC-2.3 (rule)**: 表头-行列宽严格 1:1（已在前两轮完成，此处只需确认未被破坏）

### FR-3: 详情面板卡片化分组

右侧 DetailPanel 的 5 个分组（基本信息 / 网络 / 挂载卷 / 运行配置 / Labels）改为独立卡片：

- 每个分组一个 bg-card Rectangle（border-radius 8px + 背景 #1e293b）
- SectionHeader：accent 左竖条（4px 宽，12px 高）+ 加粗文字（font-weight 600 + Theme.text-base + Theme.accent 色）
- SectionHeader 上方有 space-3 (8px) 间距
- DetailRow 在卡片内，padding-left 8px，行高 22px
- 卡片内部不画分隔线，让背景色区分

- **AC-3.1 (rule)**: 5 个 Section 全是独立 Rectangle bg-card 卡片
- **AC-3.2 (rule)**: SectionHeader 有 accent 左竖条 + 加粗 accent 色标题
- **AC-3.3 (rubric)**: 卡片分组有"树的层次感"（0-2，阈值 ≥1.5）
  - 2: 5 卡片独立可辨、间距一致、SectionHeader 与卡片顶部对齐、整体视觉有层级
  - 1: 卡片有但间距不均或 SectionHeader 没对齐
  - 0: 仍像一坨 Text

### FR-4: ToolBar icon-only 按钮行

底部 ToolBar 从三个大 FlatButton（带文字）改为三个 icon-only 小按钮（24×24px，border-radius 6px）：⏹停止全部 / ⬇拉取镜像 / 🧹清理资源。hover 时 bg-hover 变色。

- **AC-4.1 (rule)**: ToolBar 三个按钮 width: 24px height: 24px，spacing: space-3
- **AC-4.2 (rule)**: 按钮 hover 时 background = Theme.bg-hover，图标垂直水平居中
- **AC-4.3 (rubric)**: ToolBar 视觉简洁专业（0-2，阈值 ≥1）
  - 2: 三个 icon 按钮紧凑居中 + 间距 8px + 对齐 Activity Bar 右侧边缘
  - 1: 按钮大小一致但位置偏了
  - 0: 仍带文字或大小不一

### FR-5: 弹窗完全自定义

PullDialog 和 CleanupDialog 用 Slint 组件重写。结构：背景半透明遮罩 + 居中卡片（bg-surface + border-radius 10px）+ 标题区 + 内容区（padding 16px）+ footer 按钮区（右对齐）。

- **AC-5.1 (rule)**: 两个弹窗都有半透明遮罩层 + 居中卡片
- **AC-5.2 (rule)**: 弹窗标题 Theme.text-lg + Theme.text-primary + 加粗
- **AC-5.3 (rule)**: footer 按钮区右对齐（"取消" + "确认"），FlatButton 组件
- **AC-5.4 (rule)**: CleanupDialog 的 CheckBox 用 Theme.bg-card 包裹 + padding，不是原生裸样式

### FR-6: 右侧 Tab（日志/镜像详情）样式重做

右侧 DetailLogPanel 内部有自绘 TabBar（日志 / 镜像详情），当前样式粗糙。改为：accent 下划线（2px 宽 + 精确文字宽度匹配）+ active accent 色 + inactive text-muted 色。

- **AC-6.1 (rule)**: 右侧内部 Tab active 有 accent 下划线 2px + 匹配文字宽度
- **AC-6.2 (rule)**: active Tab 文字 accent 色 + 加粗；inactive Tab text-muted + 常规

### FR-7: Theme 令牌体系严格执行

所有 Text 的 color 和 font-size 必须从 Theme.text-* 令牌取，禁止硬编码 `#fff`、`white`、`#888`、`14px`、`15px` 等。所有 Rectangle 背景必须从 Theme.bg-* 取。

- **AC-7.1 (rule)**: grep -rn 'color:\s*"#' ui/*.slint → 零残留
- **AC-7.2 (rule)**: grep -rn 'font-size:\s*[0-9]*px' ui/*.slint → 零残留（必须 Theme.text-*）
- **AC-7.3 (rule)**: grep -rn 'background:\s*"#' ui/*.slint → 零残留（必须 Theme.bg-*）

### FR-8: 顶栏简化对齐

TopBar 四个元素（Logo / StatusChip / SearchInput / Refresh）水平居中对齐，高度一致（36px），spacing: space-3 (8px)。

- **AC-8.1 (rule)**: TopBar 所有元素高度 36px
- **AC-8.2 (rule)**: 元素垂直居中（vertical-alignment: center 或 parent.height 对齐）
- **AC-8.3 (rubric)**: 顶栏视觉简洁对齐（0-2，阈值 ≥1.5）
  - 2: 四个元素高度一致、垂直居中、间距均匀、整体不拥挤
  - 1: 高度一致但有轻微偏差或间距不均
  - 0: 元素错位或大小不一

## Constraints

- Slint 1.18.1：Rectangle 不能有 text/padding/border-radius/background（只能 inherits Rectangle + 内部 HorizontalBox）；HorizontalBox/VerticalBox 也不能有 border-radius/background；自定义组件属性赋值必须加分号 `;`
- Slint 没有原生 Activity Bar / Sidebar 组件，必须自绘
- software-renderer-systemfonts 不打包字体，依赖系统 Noto CJK / 文泉驿
- 单文件 ≤ 300 行
- Rust crate 版本锁定：slint 1.18.1 / slint-build 1.18.1 / bollard 0.21 / tokio rt-multi-thread / futures-util 0.3

## Assumptions

- 用户截图已确认问题全貌，无需额外用户输入即可进入 Implement
- stats 数据问题大概率是 Rust 侧 AppState.stats_prev HashMap key 类型不匹配（ContainerInfo.id vs short_id）
- Slint ListView 高度计算不变（固定 header 32px + ScrollView flex）

## Open Questions

无（用户明确说"先出方案"，不需要再交互确认）。
