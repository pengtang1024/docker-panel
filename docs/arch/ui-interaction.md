# UI 层架构：Slint 模块交互与拆分原则

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是 Slint 模块交互与 Slint 拆分原则的权威深度版。

## 1. Slint UI 模块交互

26 个 `.slint` 文件——app.slint 是唯一组装入口，子文件间**不互相 import**。

### 交互总图

```
                    app.slint（组装层 + 布局 + global-shortcuts 快捷键）
        ╱      ╱       ╱        ╲        ╲        ╲        ╲
   top-bar  sub-bar  cards   foot-bar  batch-bar  drawer  dialogs/menu-layers
   (顶栏)   (筛选)  (卡片)    (底栏)    (批量栏)   (抽屉)   (弹层)
      │        │       │
      └────────┴───┬───┴──────────────┐
                   ▼                  ▼
          components-*.slint      theme.slint（全局设计令牌）
          （9 个原子组件分组）      app-actions.slint（UI→Rust 回调 global）
                   │
                   ▼
          models.slint（ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair）
```

### 交互模式

#### ① models.slint → Rust：property 数据注入

```
models.slint struct               Rust consumer 构造
┌──────────────────────────┐      ┌─────────────────────────────┐
│ ContainerRow { id, ... } │      │ let rows = ui_rows::build_* │
│ ImageRow     { id, ... } │      │   (&s);                     │
│ NetworkRow   { ... }     │      │ render::containers(rows)    │
│ VolumeRow    { ... }     │      │   // thread_local 差量同步   │
│ DetailPair   { ... }     │      │ ui_apply::apply_drawer(&ui) │
└──────────────────────────┘      └─────────────────────────────┘
日志也是 Model：AppState.logs → LogSyncPlan(Append/Reset) → render::logs_sync → Drawer 虚拟化 ListView
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
| `app.slint` | 331 | MainWindow + global-shortcuts FocusScope + 布局组装 | theme / top-bar / sub-bar / batch-bar / cards / foot-bar / drawer / dialogs / models / app-actions |
| `app-actions.slint` | 41 | AppActions global：UI → Rust 回调声明 | 无 |
| `theme.slint` | 96 | 浅/深双主题语义色 + 字号/间距令牌，theme_mode 三态 | 无 |
| `models.slint` | 84 | ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair / LogRow | 无 |
| `top-bar.slint` | 158 | TopBar（搜索框 + 4 按钮 + focus_search） | theme / components-align / components-interact |
| `sub-bar.slint` | 194 | 筛选 chips + 排序 + 4 视图 tabs | theme / components-* / models |
| `cards.slint` | 173 | 四种资源卡片网格 + 空状态 + 背景点击层 | theme / card-* / skel / components-* |
| `card-container.slint` | 201 | 容器卡片（多选 + 启停/重启/日志） | theme / components-* |
| `card-image.slint` | 172 | 镜像卡片（运行/删除） | theme / components-* |
| `card-netvol.slint` | 138 | 网络/卷卡片（共用布局） | theme / components-* |
| `skel.slint` | 69 | 骨架屏占位 | theme |
| `foot-bar.slint` | 109 | 底部 8 项资源概览 | theme / components-align |
| `batch-bar.slint` | 39 | 批量操作浮动栏 | theme / components-* |
| `drawer.slint` | 201 | 右侧抽屉三 Tab（详情/日志/环境） | theme / drawer-detail-row / drawer-log-row / components-* |
| `drawer-detail-row.slint` | 152 | 详情/环境行（kv / 代码盒 / 挂载 chip） | theme |
| `drawer-log-row.slint` | 35 | 日志行 delegate（虚拟化 ListView 的一行 = 一条日志） | theme / components-align / models |
| `dialogs.slint` | 190 | PullDialog / CleanupDialog | theme / components-* |
| `menu-layers.slint` | 133 | 排序菜单 + 卡片「···」菜单 | theme / models |
| `components-interact.slint` | 340 | IconBtn / StateArea / TouchArea / Dropdown 等 | theme |
| `components-nav.slint` | 151 | 导航/视图切换类组件 | theme |
| `components-form.slint` | 102 | 输入/复选框等表单组件 | theme |
| `components-chip.slint` | 115 | Chip / 标签类组件 | theme |
| `components-align.slint` | 42 | VCenterText / CenterText 对齐组件 | theme |
| `components-state.slint` | 49 | StateArea 等状态层 | theme |
| `components-empty.slint` | 35 | 空态组件 | theme |
| `components-feedback.slint` | 30 | Tip / 反馈类组件 | theme |
| `components-dialog.slint` | 54 | DialogShell 基类（inherits Material Modal） | material |

## 2. Slint 拆分原则

- app.slint 做组装层——只放 MainWindow 骨架 + global-shortcuts FocusScope + import 语句 + 布局 markup
- 子文件之间不互相 import——避免循环依赖和跨文件 global 可见性问题
- 卡片/行组件用 callback 属性注入代替直接 import AppActions global，绕开 Rust 侧 glob import 兼容性坑
- 顶栏/筛选栏用 in-out 属性 `<=>` 双向绑定——数据流向一眼可见
- **TabWidget 内容必须显式 width/height**——Slint Tab 容器不传 parent 尺寸
