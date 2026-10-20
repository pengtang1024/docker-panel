# Slint UI 全面重构 — 落地 HTML 原型方案

## 问题

当前 Slint UI（Phase 2）采用固定深色主题 + 三栏布局（Sidebar + 列表 + DetailPanel），与 HTML 原型 `ui/prototype.html` 的设计方案差异巨大。需要将原型的完整设计语言（顶栏布局、卡片网格、三态主题、本地化、持久化等）落地到 Slint 1.18.1。

## 用户

Docker Panel 的最终用户（桌面 Docker 管理工具使用者）。

## 目标

将 `ui/PROTOTYPE_SPEC.md` 定义的所有视觉与交互规范完整实现到 Slint 代码，使 Slint 版本与 HTML 原型在视觉和交互上一致。

## 非目标

- 不修改 Docker 数据层（`src/docker/*`）
- 不修改消息流和线程模型
- 不增加新的 Docker 功能（仅 UI 重构）

## 约束

- Slint 1.18.1 strict 模式（`compat-1-18`），纯软件渲染
- 单文件 ≤ 300 行
- 项目名 docker-panel，零废弃旧名残留
- 现有线程模型（main + consumer + tokio runtime）不变
- 所有 Docker 结果继续走 `tx.send(Msg)` → consumer pump → `invoke_from_event_loop`

## 依赖

- `ui/PROTOTYPE_SPEC.md` — 设计规范唯一源
- `ui/prototype.html` — 可运行原型对照
- `dark-light` crate（系统主题检测，需新增依赖）
- `confy` 或 `directories` crate（配置持久化，需新增依赖）

## 假设

- Slint MCP 可用于 UI 验证
- 系统字体包含 CJK 字符（Noto CJK / 文泉驿）

## 功能需求

### F1: 三态主题系统
- 支持 auto（跟随系统）/ light / dark 三态
- 默认 auto，由系统 `prefers-color-scheme` 决定
- 所有颜色通过 `theme.slint` global property 定义
- 切换主题时所有组件即时更新

### F2: 布局重构
- 顶栏（52px）：logo + 连接状态 + 搜索 + 主题 + 刷新 + 清理 + 拉取
- 筛选栏（40px）：chips + 排序 + view tabs
- 批量栏（选中时显示）
- 卡片网格（4 列 minmax）
- 底栏（40px）：8 项概览 + 汇总
- 右侧抽屉（400px）：详情 / 日志 / 环境 三 Tab

### F3: 卡片组件
- 容器卡片：名称 + 镜像 + 端口 + 状态点 + CPU/MEM/DISK + compose标签(最右) + 操作按钮
- 镜像卡片：仓库 + tags + 统计 + 操作
- 网络卡片：名称 + driver/scope + 容器数 + 操作
- 卷卡片：名称 + 挂载 + driver/size + 操作
- 状态仅圆点，无文字
- 操作按钮纯图标 + tooltip

### F4: Tooltip 组件
- 自定义 tooltip，不用原生 title
- 跟随主题色系
- 圆角 + 阴影 + 小箭头 + 淡入动画

### F5: 本地化（i18n）
- 中/英双语
- 默认跟随系统语言，无手动切换按钮
- 所有 UI 文本走翻译函数

### F6: 配置持久化
- 主题、视图、筛选、排序、搜索、日志跟随状态持久化
- 下次启动自动恢复
- 用 confy/directories crate

### F7: 快捷键
- Ctrl+K 搜索、Ctrl+R 刷新、Ctrl+1~4 切换视图、Esc 关闭

### F8: 抽屉与对话框
- 详情面板：概览/资源/时间/网络/配置 五个 section
- 日志面板：实时日志 + 自动跟随(可暂停) + 复制 + 清空
- 环境面板：环境变量 + 端口映射 + 网络 + 挂载
- 拉取对话框、清理对话框

## 非功能需求

- 编译通过：`cargo build --release --features slint/mcp`
- clippy 无 warning：`cargo clippy --all-targets --all-features -- -D warnings`
- 无水平滚动条（抽屉、日志、底栏）
- 主题切换无闪烁

## 验收标准

### AC1: 主题系统
- **rule**: `theme.slint` 定义浅色/深色两套令牌，通过全局 property 切换
- **rule**: 默认不设置主题时跟随系统 `prefers-color-scheme`
- **rule**: 主题按钮循环 auto→light→dark→auto
- **rubric**: 所有组件颜色跟随主题切换，无硬编码色值残留（compose项目色除外），评分 0-2，阈值 ≥1.5

### AC2: 布局
- **rule**: 顶栏包含 logo/连接状态/搜索/主题/刷新/清理/拉取
- **rule**: 筛选栏包含 chips/排序/view tabs
- **rule**: 底栏包含 8 项概览统计
- **rule**: 右侧抽屉 400px 宽，三 Tab
- **rubric**: 整体布局与 prototype.html 视觉一致，评分 0-2，阈值 ≥1.5

### AC3: 卡片
- **rule**: 容器卡片 compose 标签在资源指标行最右侧
- **rule**: 状态仅圆点，无文字
- **rule**: 操作按钮纯图标 + tooltip
- **rubric**: 四种卡片（容器/镜像/网络/卷）视觉与原型一致，评分 0-2，阈值 ≥1.5

### AC4: Tooltip
- **rule**: tooltip 跟随主题色系（浅主题浅背景，深主题深背景）
- **rule**: tooltip 有圆角、阴影、小箭头
- **rule**: 所有操作按钮有 tooltip

### AC5: 本地化
- **rule**: 系统语言为中文时显示中文，否则英文
- **rule**: 无语言切换按钮
- **rule**: 所有 UI 文本可翻译

### AC6: 持久化
- **rule**: 主题/视图/筛选/排序/搜索/日志跟随状态保存到配置文件
- **rule**: 重启后状态恢复

### AC7: 快捷键
- **rule**: Ctrl+K 聚焦搜索
- **rule**: Ctrl+R 刷新
- **rule**: Ctrl+1~4 切换视图
- **rule**: Esc 关闭抽屉/对话框

### AC8: 编译与质量
- **rule**: `cargo build --release --features slint/mcp` 成功
- **rule**: `cargo clippy --all-targets --all-features -- -D warnings` 无 warning
- **rule**: `cargo fmt --all -- --check` 通过
