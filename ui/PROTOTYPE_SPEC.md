# Docker Panel UI 原型方案（v0.3）

> 本文件为 HTML 原型 `ui/prototype.html` 的设计定稿，作为 Slint 落地的唯一视觉与交互规范源。
> 所有组件、颜色、间距、交互行为均以此文档为准。

---

## 1. 设计系统

### 1.1 主题模式

三态主题，默认 **auto（跟随系统）**：

| 模式 | data-theme | 说明 |
|------|-----------|------|
| auto | 不设置 | 由 `@media (prefers-color-scheme)` 决定 |
| light | `data-theme="light"` | 强制浅色 |
| dark | `data-theme="dark"` | 强制深色 |

切换循环：`auto → light → dark → auto`，图标：`monitor → sun → moon`。

### 1.2 颜色令牌

所有颜色通过 CSS 变量定义，深浅主题各一套。

**浅色主题**：
```
--bg:#f7f7f8        页面背景
--card:#fff         卡片背景
--line:#ebeaee      边框
--line-2:#f4f4f5    次级边框/分隔
--ink:#0e0e11       主文字
--ink-2:#52525b     次级文字
--ink-3:#a1a1aa     辅助文字
--ink-4:#d4d4d8     禁用文字
--ok:#16a34a        成功/运行
--stop:#a1a1aa      停止
--warn:#d97706      警告
--err:#dc2626       错误
--bar-cpu:#3b82f6   CPU 蓝
--bar-mem:#8b5cf6   内存紫
--log-bg:#f4f4f5    日志背景
--log-ink:#0e0e11   日志文字
--tip-bg:#fff       tooltip 背景
--tip-ink:#0e0e11   tooltip 文字
```

**深色主题**：
```
--bg:#0c0c0e  --card:#18181b  --line:#27272a  --line-2:#1f1f23
--ink:#fafafa --ink-2:#a1a1aa --ink-3:#71717a --ink-4:#3f3f46
--ok:#4ade80  --stop:#71717a  --warn:#fbbf24  --err:#f87171
--bar-cpu:#60a5fa --bar-mem:#a78bfa
--log-bg:#000 --log-ink:#e4e4e7
--tip-bg:#18181b --tip-ink:#fafafa
```

### 1.3 字体 / 圆角 / 间距

```
--f: system-ui, "PingFang SC", "Microsoft YaHei", sans-serif   UI 字体
--m: ui-monospace, "JetBrains Mono", Menlo, monospace          等宽字体
--r-xs:4px  --r-sm:6px  --r-md:8px  --r:14px
```

---

## 2. 布局结构

```
┌─────────────────────────────────────────────────┐
│ TopBar: logo · 连接状态 · 搜索 · 主题 · 刷新 · 清理 · 拉取 │
├─────────────────────────────────────────────────┤
│ SubBar: chips(筛选) ──── sort(排序) ──── viewTabs │
├─────────────────────────────────────────────────┤
│ BatchBar: 已选 N 个 ──── 启动/停止/重启/删除/取消    │  (选中时显示)
├─────────────────────────────────────────────────┤
│                                                 │
│              卡片网格 (4 列 minmax)               │
│                                                 │
├─────────────────────────────────────────────────┤
│ FootBar: ● 状态 ──── 8项概览 ──── 汇总            │
└─────────────────────────────────────────────────┘

右侧抽屉 (400px): 详情 / 日志 / 环境 三 Tab
```

### 2.1 顶栏 TopBar

- 高度 52px，`var(--card)` 背景，底部边框
- 搜索框：放大镜图标 + input + `Ctrl+K` 快捷键提示
- 按钮：主题(monitor)、刷新、清理(trash)、拉取(download)
- 所有按钮纯图标 + tooltip，无文字

### 2.2 筛选栏 SubBar

- 高度 40px
- 左侧 chips：全部 / 运行中 / 已停止 / compose 项目名，带数量角标
- 右侧排序按钮：图标 + 当前字段名 + 方向箭头
  - 点**方向箭头**：直接翻转排序方向（降序 ⇵ / 升序 ⇅），不开菜单
  - 点**按钮其余部分**：展开排序菜单（选字段；重选当前字段 = 翻转方向）
- view tabs：容器 / 镜像 / 网络 / 卷，带数量

### 2.3 批量栏 BatchBar

- 仅当有卡片被选中时显示
- 左侧："已选 N 个"
- 右侧操作按钮：启动 / 停止 / 重启 / 删除 / 取消
- 全图标 + tooltip

### 2.4 底栏 FootBar

- 高度 40px，`overflow-x:auto` 但滚动条隐藏
- 8 项概览：Docker 版本 / 容器 / 镜像 / 网络 / 卷 / CPU / 内存 / 磁盘
- 每项：图标 + 数值 + 标签，`flex-shrink:1` 可压缩，标签 `max-width:60px` 省略
- 右侧汇总文本

---

## 3. 组件规范

### 3.1 容器卡片

```
.card (border, radius 14px, hover shadow)
├─ .check        复选框 (sel-mode 时显示)
├─ .c-name       名称 (14px bold)
├─ .c-img        镜像 (11px mono, ink-3)
├─ .c-ports      端口 (最多3个 +N)
├─ .c-res        资源行
│   ├─ status-dot  状态点 (绿/灰, hover tooltip)
│   ├─ CPU 指标    cpu图标 + 数值
│   ├─ MEM 指标    memory图标 + 数值
│   ├─ DISK 指标   disk图标 + 数值
│   └─ proj-tag    compose 项目标签 (margin-left:auto, 按项目着色)
└─ .card-actions
    ├─ 主按钮 (运行中→restart, 停止→play)
    └─ 更多菜单 (停止/日志/删除)
```

**关键约束**：
- compose 标签在资源行最右侧，不在卡片底部
- 状态仅圆点，无文字
- 资源指标图标与数值 `align-items:center; line-height:1`

### 3.2 镜像卡片

```
.img-card
├─ .repo        仓库名
├─ .tags        tag 列表 (latest 高亮)
├─ .img-stats   大小 / 层数 / 创建
└─ .card-actions  运行 / 详情 / 删除
```

### 3.3 网络卡片

```
.net-card
├─ .net-name    名称 (+内置标签)
├─ .net-sub     driver · scope · subnet
├─ .net-tags    driver / scope chip
├─ .net-foot    N 个容器
└─ .card-actions
```

### 3.4 卷卡片

```
.vol-card
├─ .vol-name    名称 (匿名卷截断+tooltip)
├─ .anon-tag    匿名卷标签
├─ .vol-mount   挂载路径
├─ .vol-foot    driver / size
└─ .card-actions
```

### 3.5 Tooltip 组件

- 自定义实现，不用原生 `title`
- 背景 `var(--tip-bg)`，文字 `var(--tip-ink)`，圆角 4px，阴影
- 底部小三角箭头，`var(--tip-arrow)`
- 淡入动画 120ms，延迟 300ms 显示
- 跟随主题色系（浅主题浅背景，深主题深背景）
- 所有交互元素用 `data-tip` 属性

### 3.6 状态点

- 运行：`var(--ok)` 绿色圆点
- 停止：`var(--stop)` 灰色圆点
- 仅图标，无文字，hover 显示 tooltip

### 3.7 资源进度条

- CPU 蓝 `var(--bar-cpu)`，内存紫 `var(--bar-mem)`
- 背景 `var(--bar-bg)`

---

## 4. 交互规范

### 4.1 卡片操作

| 元素 | 交互 |
|------|------|
| 卡片主体 | 点击选中/取消选中（复选模式） |
| 主按钮 | 运行中→重启，停止→启动 |
| 更多菜单 | 停止 / 日志 / 删除 |
| 状态点 | hover tooltip 显示运行中/已停止 |

### 4.2 抽屉

- 从右侧滑入，400px 宽
- 三 Tab：详情 / 日志 / 环境
- 日志面板：实时日志，自动跟随滚动（可暂停），复制，清空
- 环境面板：环境变量（键值高亮）、端口映射、网络连接、挂载
- 内容区 `overflow-y:auto; overflow-x:hidden`，无水平滚动条

### 4.3 对话框

- 拉取镜像：输入框 + 进度条 + 状态文本
- 清理资源：4 项 checkbox 列表（停止容器/悬空镜像/未用网络/未用卷）+ 数量

### 4.4 快捷键

| 按键 | 功能 |
|------|------|
| Ctrl+K | 聚焦搜索 |
| Ctrl+R | 刷新 |
| Ctrl+1~4 | 切换视图（容器/镜像/网络/卷） |
| Esc | 关闭抽屉/对话框 |
| / | 聚焦搜索 |

---

## 5. 主题系统

### 5.1 实现方式

- `:root`（无 data-theme）由媒体查询决定深浅
- `[data-theme="light"]` / `[data-theme="dark"]` 强制覆盖
- 所有颜色走 CSS 变量，无硬编码色值（compose 项目色除外）

### 5.2 跟随系统的组件

| 组件 | 浅色 | 深色 |
|------|------|------|
| 页面背景 | #f7f7f8 | #0c0c0e |
| 卡片 | #fff | #18181b |
| 文字 | #0e0e11 | #fafafa |
| Tooltip | #fff 背景 + 深色字 | #18181b 背景 + 浅色字 |
| 日志面板 | #f4f4f5 背景 + 深色字 | #000 背景 + 浅色字 |
| 状态色 | ok/warn/err/stop | 提亮版 |
| 进度条 | 蓝/紫 | 亮蓝/亮紫 |

---

## 6. 本地化（i18n）

### 6.1 语言策略

- **默认跟随系统语言**，无手动切换按钮
- `navigator.language.startsWith('zh')` → 中文，否则英文
- 不存 localStorage，每次启动重新检测

### 6.2 实现方式

- `I18N` 字典：`{ zh: {...}, en: {...} }`，约 80 个 key
- `t(key)` 函数获取翻译
- 静态 HTML：`data-i18n` / `data-i18n-tip` / `data-i18n-ph` 属性
- 动态内容：直接 `t('key')` 调用
- `applyLang()` 统一替换静态文本

### 6.3 不翻译的内容

- 技术术语：PID、CPU、Labels、Docker
- 容器名、镜像名、端口号等数据
- compose 项目名

---

## 7. 配置持久化

所有用户操作和配置状态通过 `localStorage` 持久化，下次进入自动恢复。

### 7.1 持久化项

| 键 | 含义 | 取值 | 默认 |
|----|------|------|------|
| `dp-theme` | 主题模式 | auto / light / dark | auto |
| `dp-view` | 当前视图 | c / i / n / v | c |
| `dp-filter` | 筛选 chip | all / running / stopped / compose名 | all |
| `dp-sort-field` | 排序字段 | name / cpu / mem / disk / on | name |
| `dp-sort-desc` | 排序方向 | 1(降序) / 0(升序) | 1 |
| `dp-search` | 搜索关键词 | 任意字符串 | 空 |
| `dp-log-follow` | 日志自动跟随 | 1(开) / 0(关) | 1 |

### 7.2 实现方式

- `saveConfig()`：状态变更时统一写入 localStorage
- `loadConfig()`：初始化时读取并校验合法性（枚举值白名单校验）
- 触发保存的函数：`setView` / `setChip` / `doSearch` / `setSortField` / `toggleLogFollow` / `toggleTheme`
- 语言不持久化，每次跟随系统语言

### 7.3 不持久化的内容

- 批量选择状态（临时操作）
- 对话框开关状态
- 抽屉打开状态
- 折叠分组（动态数据）

---

## 8. 已知限制

- 无快捷键自定义
- 无 Docker Compose 导入导出
- 镜像行不支持多选批量
- 仅中/英双语

---

## 9. 落地到 Slint 的映射要点

| HTML 原型 | Slint 实现 |
|-----------|-----------|
| CSS 变量 | `theme.slint` global out property |
| data-i18n | Rust 侧 `t!` 宏或字符串替换 |
| 卡片网格 | `GridLayout` + `for` 循环 |
| tooltip | 自绘 Popup 或 `Tooltip` 组件 |
| 主题切换 | `AppActions.set_theme()` + 全局 property |
| 系统主题 | Slint 暂无原生 API，需 Rust 侧 `dark-light` crate 检测 |
| 抽屉动画 | `animate` transition |
| localStorage | Rust 侧 `confy` / `directories` crate 读写配置文件 |
