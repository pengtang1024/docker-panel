# UI 层架构：Slint 模块交互与拆分原则

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是 Slint 模块交互与 Slint 拆分原则的权威深度版。

## 1. Slint UI 模块交互

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

## 2. Slint 拆分原则

- app.slint 做组装层——只放 MainWindow 骨架 + global-shortcuts FocusScope + import 语句 + 布局 markup
- 子文件之间不互相 import——避免循环依赖和跨文件 global 可见性问题
- 卡片/行组件用 callback 属性注入代替直接 import AppActions global，绕开 Rust 侧 glob import 兼容性坑
- 顶栏/筛选栏用 in-out 属性 `<=>` 双向绑定——数据流向一眼可见
- **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸
