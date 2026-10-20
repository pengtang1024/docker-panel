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

## 3. 两条容易踩的 Slint 规则

### 3.1 命中顺序：TouchArea 声明在布局之后 = 盖住布局里的热区

Slint 按**声明顺序**做命中测试：后声明的兄弟节点画在上面，也先拿到指针事件。
所以一个按钮的「整块热区」TouchArea 若声明在 HorizontalLayout **之后**，就会盖住布局内部
自己声明的 TouchArea——布局里的热区永远收不到点击。

本项目的写法约定（`Chip` / `ViewTab` / 排序按钮统一）：

```slint
component Chip inherits Rectangle {
    area := StateArea { clicked => { root.clicked(root.key); } }   // ← 热区在前
    HorizontalLayout { ... }                                     // ← 内容在后
}
```

- 热区在前 → 布局内的独立热区（排序按钮的方向箭头）能被点到；
  布局里的 `Text` 没有 input region，点击会**穿透**到下面的热区（打开菜单）。
- 热区在后 → 布局内的热区是死代码（点箭头只会触发外层热区）。

> 踩过的坑：排序按钮的外层 TouchArea 声明在布局之后 → 方向箭头的
> `set_sort_dir()` 永远不触发，点箭头只会开菜单、方向翻不动。

### 3.2 UI 状态单一写入方：回调写，consumer 不回写

Slint 属性（如 `sort_field` / `sort_desc` / `filter` / `view`）由 **UI 回调**独占写入：
回调在 UI 线程里同步改 `AppState` → 立刻 `ui.set_xxx(...)` → 重建受影响的列表。

consumer 的 3s 心跳**不得**回写这些属性：它在锁内取快照、锁外才写 UI，
期间用户改了条件的话，回写的就是**旧值**——排序状态与箭头会偶发对不上
（箭头滞后最多 3s，甚至被旧快照打回旧方向）。

consumer 只负责「按 Msg 增量刷新数据」，不负责「回写用户操作过的状态」。
需要防的另一种竞态（锁内建行、锁外应用，期间条件被改）用
`AppState::rows_epoch` 判定：epoch 变了就丢弃这批旧行（回调已用新条件重建过）。

## 4. 窗口身份（xdg app_id）与 Dock 图标

桌面环境靠「窗口身份」把窗口关联到 `.desktop` 文件，再决定 Dock（面板坞）里显示什么图标和名字。

| 后端 | 机制 | 不设置的后果 |
|------|------|--------------|
| Wayland | toplevel 的 `app_id`（`xdg_toplevel.set_app_id`） | GNOME Shell 匹配不到 `docker-panel.desktop` → Dock 显示「未识别程序」，图标退化成通用占位图 |
| X11 | `WM_CLASS` 的 class 段 | winit 用 argv[0] 回退 → class = `docker-panel`，与 `.desktop` 的 `StartupWMClass` 一致 → 正常 |

Slint 1.18 提供了 `slint::set_xdg_app_id(app_id)`，但**不主动调用**（`i-slint-backend-winit` 只在
`WindowInner::xdg_app_id()` 有值时才调 `with_name(app_id, "")`）。所以本项目在 `src/main.rs` 里显式设置：

```rust
// Wayland：app_id = desktop 文件名去掉 .desktop 后缀
if std::env::var_os("WAYLAND_DISPLAY").is_some() {
    let _ = slint::set_xdg_app_id("docker-panel");
}
```

三个关键约束：

1. **时机**——必须早于窗口 show。native window 在 `ui.run()` 里才创建（`ensure_window`），
   所以放在 `MainWindow::new()` 之后、`ui.run()` 之前即可。
2. **只在 Wayland 下设置**——Slint 对 X11 也用 `with_name(app_id, "")`，会让 `WM_CLASS` 的 class 段变空，
   反而破坏 X11 下的匹配。
3. **取值必须是 desktop 文件基名**——`docker-panel` → 查 `docker-panel.desktop` → `Icon=docker-panel` →
   `hicolor/scalable/apps/docker-panel.svg`。

验证方法（不依赖截图）：

```bash
WAYLAND_DEBUG=1 ./target/release/docker-panel 2>wl.log
grep set_app_id wl.log     # 应有 xdg_toplevel#N.set_app_id("docker-panel")
```
