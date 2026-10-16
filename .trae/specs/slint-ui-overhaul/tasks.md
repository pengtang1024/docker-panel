# 实现任务队列

## Task 1: 主题系统重构
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC1
- **描述**: 重构 `theme.slint`，定义浅色/深色两套令牌，支持三态切换（auto/light/dark）
- **工作**:
  1. `theme.slint` 改为 `global Theme`，定义所有颜色/字号/间距的 `in-out property`
  2. 新增 `theme_mode` property（0=auto, 1=light, 2=dark）
  3. 根据 mode 选择对应的令牌值
  4. Rust 侧用 `dark-light` crate 检测系统主题，auto 模式下应用
  5. 主题切换按钮循环切换
- **TR**:
  - rule: theme.slint 编译通过，无语法错误
  - rule: 切换主题后所有颜色 property 更新
  - rubric: 颜色令牌与 PROTOTYPE_SPEC.md 一致（浅色 #f7f7f8/#fff/#0e0e11，深色 #0c0c0e/#18181b/#fafafa），评分 0-2，阈值 ≥1.5

## Task 2: 布局骨架重构
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC2
- **依赖**: Task 1
- **描述**: 重构 `app.slint` 主窗口布局，从三栏改为顶栏+筛选栏+卡片网格+底栏+右侧抽屉
- **工作**:
  1. 移除 Sidebar 和内嵌 DetailPanel
  2. 顶栏 52px + 筛选栏 40px + 批量栏(可选) + 卡片网格(stretch) + 底栏 40px
  3. 右侧抽屉 400px，默认隐藏，打开时滑入
  4. 定义 MainWindow 的 in-out properties（view, filter, sort_field, sort_desc, search_q, log_follow 等）
- **TR**:
  - rule: app.slint 编译通过
  - rule: 布局结构为 VerticalLayout(TopBar, SubBar, BatchBar?, GridArea, FootBar) + Drawer
  - rubric: 布局比例与原型一致，评分 0-2，阈值 ≥1.5

## Task 3: 顶栏组件 TopBar
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC2, AC4
- **依赖**: Task 1
- **描述**: 实现顶栏：logo + 连接状态 + 搜索框 + 主题/刷新/清理/拉取按钮
- **工作**:
  1. 新建 `top-bar.slint` 或重构 `panes.slint` 的 TopBar
  2. 搜索框带 Ctrl+K 提示
  3. 所有按钮纯图标 + tooltip callback
  4. 连接状态用圆点 + 文本
- **TR**:
  - rule: 顶栏包含 7 个元素（logo, 状态, 搜索, 主题, 刷新, 清理, 拉取）
  - rule: 搜索框可输入并触发搜索回调
  - rubric: 视觉与原型一致，评分 0-2，阈值 ≥1.5

## Task 4: 筛选栏组件 SubBar
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC2
- **依赖**: Task 1
- **描述**: 实现筛选栏：chips + 排序按钮 + view tabs
- **工作**:
  1. chips：全部/运行中/已停止/compose项目，带数量
  2. 排序按钮：图标 + 字段名 + 方向箭头，点击展开菜单
  3. view tabs：容器/镜像/网络/卷，带数量
  4. 状态通过 in-out property 双向绑定
- **TR**:
  - rule: chips 可切换，选中状态高亮
  - rule: 排序菜单可展开选择字段
  - rule: view tabs 可切换四个视图
  - rubric: 视觉与原型一致，评分 0-2，阈值 ≥1.5

## Task 5: 卡片组件
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC3
- **依赖**: Task 1
- **描述**: 实现四种卡片：容器/镜像/网络/卷
- **工作**:
  1. 容器卡片：名称 + 镜像 + 端口(最多3+N) + 状态点 + CPU/MEM/DISK + compose标签(最右) + 主按钮 + 更多菜单
  2. 镜像卡片：仓库 + tags + 大小/层数/创建 + 操作
  3. 网络卡片：名称 + driver/scope + 容器数 + 操作
  4. 卷卡片：名称 + 挂载 + driver/size + 操作
  5. 卡片网格 4 列 minmax
- **TR**:
  - rule: 容器卡片 compose 标签在资源行最右
  - rule: 状态仅圆点
  - rule: 操作按钮纯图标
  - rubric: 四种卡片视觉与原型一致，评分 0-2，阈值 ≥1.5

## Task 6: 底栏组件 FootBar
- **Priority**: medium
- **Status**: completed
- **AC 覆盖**: AC2
- **依赖**: Task 1
- **描述**: 实现底栏 8 项概览：Docker版本/容器/镜像/网络/卷/CPU/内存/磁盘 + 汇总
- **工作**:
  1. 每项：图标 + 数值 + 标签
  2. 可压缩（flex-shrink:1），标签可省略
  3. 无水平滚动条
- **TR**:
  - rule: 底栏显示 8 项概览
  - rule: 无水平滚动条
  - rubric: 视觉与原型一致，评分 0-2，阈值 ≥1.5

## Task 7: Tooltip 组件
- **Priority**: medium
- **Status**: completed
- **AC 覆盖**: AC4
- **依赖**: Task 1
- **描述**: 实现自定义 tooltip，跟随主题色系
- **工作**:
  1. 新建 `tooltip.slint`，定义 Tooltip component
  2. 背景/文字/箭头跟随主题
  3. 圆角 + 阴影 + 小箭头 + 淡入动画
  4. 所有按钮支持 tooltip
- **TR**:
  - rule: tooltip 背景色 = 主题 tip-bg
  - rule: tooltip 有圆角和阴影
  - rule: 所有操作按钮显示 tooltip

## Task 8: 抽屉组件 Drawer
- **Priority**: high
- **Status**: completed
- **AC 覆盖**: AC2, AC8
- **依赖**: Task 1
- **描述**: 实现右侧抽屉，三 Tab：详情/日志/环境
- **工作**:
  1. 详情：概览/资源/时间/网络/配置 section，kv 行
  2. 日志：实时日志 + 自动跟随(可暂停) + 复制 + 清空
  3. 环境：环境变量(键值高亮) + 端口映射 + 网络 + 挂载
  4. 无水平滚动条
- **TR**:
  - rule: 抽屉 400px 宽，从右滑入
  - rule: 三 Tab 可切换
  - rule: 日志面板支持自动跟随开关
  - rule: 无水平滚动条
  - rubric: 视觉与原型一致，评分 0-2，阈值 ≥1.5

## Task 9: 对话框
- **Priority**: medium
- **Status**: completed
- **AC 覆盖**: AC8
- **依赖**: Task 1
- **描述**: 实现拉取镜像和清理资源对话框
- **工作**:
  1. 拉取对话框：输入框 + 进度条 + 状态
  2. 清理对话框：4 项 checkbox + 数量
- **TR**:
  - rule: 拉取对话框可输入并显示进度
  - rule: 清理对话框可勾选 4 项资源

## Task 10: 本地化 i18n
- **Priority**: medium
- **Status**: pending
- **AC 覆盖**: AC5
- **依赖**: Task 3-9
- **描述**: 实现中/英双语，默认跟随系统语言
- **工作**:
  1. Rust 侧定义 I18N 字典（zh/en）
  2. 检测系统语言
  3. 所有 UI 文本走翻译函数
  4. 无语言切换按钮
- **TR**:
  - rule: 系统语言中文时显示中文
  - rule: 系统语言非中文时显示英文
  - rule: 无语言切换按钮

## Task 11: 配置持久化
- **Priority**: medium
- **Status**: pending
- **AC 覆盖**: AC6
- **依赖**: Task 2-10
- **描述**: 用 confy/directories crate 持久化配置
- **工作**:
  1. 定义 Config 结构体（theme, view, filter, sort_field, sort_desc, search, log_follow）
  2. 启动时加载，变更时保存
  3. 枚举值校验
- **TR**:
  - rule: 配置保存到用户配置目录
  - rule: 重启后状态恢复

## Task 12: 快捷键
- **Priority**: low
- **Status**: pending
- **AC 覆盖**: AC7
- **依赖**: Task 2-9
- **描述**: 实现全局快捷键
- **工作**:
  1. Slint `Shortcut` 或 Rust 侧全局快捷键
  2. Ctrl+K 搜索、Ctrl+R 刷新、Ctrl+1~4 视图、Esc 关闭
- **TR**:
  - rule: Ctrl+K 聚焦搜索
  - rule: Ctrl+R 刷新
  - rule: Ctrl+1~4 切换视图
  - rule: Esc 关闭抽屉/对话框

## Task 13: 编译验证与 MCP 验收
- **Priority**: high
- **Status**: pending
- **AC 覆盖**: AC1-AC8
- **依赖**: Task 1-12
- **描述**: 编译验证 + Slint MCP 截图对照原型验收
- **工作**:
  1. cargo build --release --features slint/mcp
  2. cargo clippy --all-targets --all-features -- -D warnings
  3. cargo fmt --all -- --check
  4. 启动带 MCP 的版本，截图对照 prototype.html
  5. 修复所有视觉差异
- **TR**:
  - rule: build 成功
  - rule: clippy 无 warning
  - rule: fmt 通过
  - rubric: MCP 截图与原型视觉一致，评分 0-2，阈值 ≥1.5
