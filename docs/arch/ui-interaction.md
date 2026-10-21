# UI 层架构：Slint 模块交互与拆分原则

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是 Slint 模块交互与 Slint 拆分原则的权威深度版。

## 1. Slint UI 模块交互

33 个 `.slint` 文件——app.slint 是唯一组装入口，子文件间**不互相 import**。

### 交互总图

```
                    app.slint（组装层 + 布局 + global-shortcuts 快捷键）
        ╱      ╱       ╱        ╲        ╲        ╲        ╲
   top-bar  sub-bar  list-area  foot-bar  batch-bar  drawer  dialogs/menu-layers
   (顶栏)   (筛选)   (列表)      (底栏)    (批量栏)   (抽屉)   (弹层)
      │        │       │
      └────────┴───┬───┴──────────────┐
                   ▼                  ▼
          components-*.slint      theme.slint（全局设计令牌）
          （9 个原子组件分组）      app-actions.slint（UI→Rust 回调 global）
                   │
                   ▼
          models.slint（ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair / LedgerSummary）
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

#### ② 列表/行组件 ← app.slint：callback 属性注入

列表与行组件不 import AppActions global——只声明 `callback toggle(string)` 等。
app.slint / list-area.slint 组装处注入实现转发到 AppActions。

#### ③ top-bar / sub-bar ↔ app.slint：`<=>` 双向绑定

`search_q <=> root.search_q` / `docker_ok <=> root.docker_ok` / `view <=> root.view`

#### ④ Drawer：条件渲染 + 自绘 Tab

`if root.drawer_open : Drawer {}` 条件渲染；抽屉内详情/日志/环境三 Tab 自绘 TabBar 切换。
**Esc 关闭弹层后焦点悬空**——app.slint Esc 分支末尾调 `global-shortcuts.focus()` 归还焦点。

### Slint 文件清单速查

| 文件 | 行数 | 职责 | import 谁 |
|------|------|------|-----------|
| `app.slint` | 368 | MainWindow + global-shortcuts FocusScope + 布局组装 + M3 色板接线 | theme / material-theme / top-bar / main-area / foot-bar / drawer / dialogs / models / app-actions |
| `app-actions.slint` | 41 | AppActions global：UI → Rust 回调声明 | 无 |
| `theme.slint` | 97 | 浅/深双主题语义色 + 字号/间距令牌，theme_mode 三态（`is_dark` 为 out，供 material-theme 消费） | 无 |
| `material-theme.slint` | 89 | M3 色板映射：Theme.* → MaterialScheme（53 个角色全覆盖） | theme / @material |
| `models.slint` | 100 | ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair / LogRow / LedgerSummary | 无 |
| `top-bar.slint` | 158 | TopBar（搜索框 + 4 按钮 + focus_search） | theme / components-align / components-interact |
| `sub-bar.slint` | 197 | 状态筛选 chips + 排序 + 4 视图 tabs | theme / components-* / models |
| `main-area.slint` | 166 | 主列表工作区：SubBar + BatchBar + ListArea | theme / sub-bar / batch-bar / list-area / models |
| `list-area.slint` | 158 | 资源列表区：容器视图走台账，镜像/网络/卷走表格列表 + 空状态 + 背景点击层 | theme / ledger / image-list / network-list / volume-list / skel / components-* |
| `ledger.slint` | 253 | 容器台账区：列宽统一计算 + 表头（可点击排序）+ @material ListView 虚拟化行 | theme / ledger-row / ledger-band / ledger-cols / models / components-* |
| `ledger-row.slint` | 217 | 台账行 delegate（30px）：状态轨 / 勾选框 / 微条 / 行内操作 | theme / ledger-cols / models / components-* |
| `ledger-band.slint` | 116 | 台账概览带：4 个计数 + CPU 占用 TOP4 | theme / components-chip |
| `ledger-cols.slint` | 23 | 台账列宽契约 global（表头与行共用同一组列宽） | 无 |
| `ledger-tree-row.slint` | 124 | 台账内联树分组带 delegate（项目带 / 服务带：箭头 + 色点 + 计数 + 组内资源汇总，通栏，列结构镜像 ledger-row） | theme / ledger-cols / models / components-* |
| `image-list.slint` | 217 | 镜像表格列表：repo:tag · 大小 · 创建时间（表头可点排序） | theme / components-list / components-interact / models / @material |
| `network-list.slint` | 157 | 网络表格列表：名称 · driver · 子网（表头可点排序） | theme / components-list / models / @material |
| `volume-list.slint` | 149 | 卷表格列表：名称 · 大小 · 挂载点（表头可点排序） | theme / components-list / models / @material |
| `components-list.slint` | 107 | 列表共用基件：ListCols 列宽契约 + ListHeadCell 表头单元格 | theme / components-align |
| `skel.slint` | 69 | 骨架屏占位 | theme |
| `foot-bar.slint` | 109 | 底部 8 项资源概览 | theme / components-align |
| `batch-bar.slint` | 39 | 批量操作浮动栏 | theme / components-* |
| `drawer.slint` | 201 | 右侧抽屉三 Tab（详情/日志/环境） | theme / drawer-detail-row / drawer-log-row / components-* / @material（ScrollView+ListView） |
| `drawer-detail-row.slint` | 152 | 详情/环境行（kv / 代码盒 / 挂载 chip） | theme |
| `drawer-log-row.slint` | 35 | 日志行 delegate（虚拟化 ListView 的一行 = 一条日志） | theme / components-align / models |
| `dialogs.slint` | 190 | PullDialog / CleanupDialog | theme / components-* |
| `menu-layers.slint` | 133 | 排序菜单 + 卡片「···」菜单 | theme / models |
| `components-interact.slint` | 340 | IconBtn / StateArea / TouchArea / Dropdown 等 | theme |
| `components-nav.slint` | 151 | 导航/视图切换类组件 | theme |
| `components-form.slint` | 102 | 输入/复选框等表单组件 | theme |
| `components-chip.slint` | 92 | Chip / MonoChip / MicroBar 等数据展示组件 | theme |
| `components-align.slint` | 42 | VCenterText / CenterText 对齐组件 | theme |
| `components-state.slint` | 49 | StateArea 等状态层 | theme |
| `components-empty.slint` | 35 | 空态组件 | theme |
| `components-feedback.slint` | 30 | Tip / 反馈类组件 | theme |
| `components-dialog.slint` | 54 | DialogShell 基类（手搓 inherits Rectangle，未用 Material Modal） | theme |

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

### 3.3 密集表格：弹性列的宽度不能交给 Slint 布局分配

台账（`ledger.slint`）的核心承诺是**列起点固定**：表头与每一行逐列同 x 同宽，
CPU / 内存两列能竖着扫读比较。

Slint 的 `HorizontalLayout` 分配空间时，**先给每个子项「首选宽度」（文本的自然宽度）**，
再把剩余空间按 `horizontal-stretch` 比例分。子项是 `Text` 时，首选宽度 = 文本多长
→ 容器名一长，名称列就被撑宽 → CPU / 内存列的起点**逐行漂移**，表格直接失去意义。

正确写法：**由父级统一算出列宽，显式下发给表头与行**。

```slint
// ledger.slint：LedgerArea 统一算，两个弹性列平分剩余宽度
property <length> content_w: root.width - LedgerCols.scrollbar_slot - LedgerCols.pad_x * 2;
property <length> name_w: (root.content_w - root.fixed_w - root.gaps_w) / 2;
property <length> image_w: (root.content_w - root.fixed_w - root.gaps_w) / 2;
// 表头与行都用显式宽度（不用 horizontal-stretch）
VCenterText { text: root.data.name; width: root.name_w; horizontal-stretch: 0; overflow: elide; }
```

配套的两条约束：

- **表头在滚动区外、行在滚动区内**：行列表视口比区域窄一个滚动条槽（8px），
  所以表头的 `padding-right` 要额外加上 `scrollbar_slot`，表头内容宽才与行内容宽相等。
- **响应式列显隐由父级统一判定**（`show_disk` / `show_ports`），
  表头与行共用同一组判定，隐列时列数与间距数同步变化，不会出现「表头 10 列、行 9 列」。

验收方式：MCP `get_element_tree` 取表头与各行的 `absolutePosition.x` / `size.width`，
逐列比对必须完全相等（不要靠肉眼读截图，12px 的偏移肉眼很难发现）。

### 3.4 台账内联树（Compose）：分组是分组，不是筛选器

Compose 层级改为**台账内联树**（`ledger-tree-row.slint`），不再有左侧栏。
踩过 / 固化下来的四条：

| 坑 | 正确做法 |
|----|----------|
| **分组带一行两个热区**（展开箭头 + 分组名） | 两个热区**互不重叠**：箭头落在状态点列内（按 `depth*18` 缩进），分组名热区从名称起点开始 → 谁也不抢谁的事件（§3.1 命中顺序铁律） |
| **分组带通栏，不参与列宽计算** | 分组带是独立 delegate（`kind = 1/2`），不占用台账的列契约；否则「项目名」会被算成一列，表格列起点漂移 |
| **分组名要与同层容器行的名称列起点对齐** | 箭头占 14px（与状态点列同槽）+ 色点 18px → 分组名落在 `44px + depth*18`，与同层容器行名称列**逐像素对齐**；层级只靠缩进表达，不靠「分组名比孩子更右」的半格错位 |
| **分组是分组不是筛选器** | 排序字段只决定**组内**顺序，组间按组名稳定排；展开状态存「**收起**的分组键」集合（`project:<名>` / `svc:<项目>/<服务>`），不在集合里 = 展开 → 新分组默认展开 |
| **分组带要显示资源汇总** | 汇总 = **组内成员之和**（CPU / 内存只算运行中成员，磁盘算全部成员），落在与容器行**同 x** 的 CPU / 内存 / 磁盘列 → 分组带内部必须**镜像 `ledger-row.slint` 的列结构与列宽**（同一套 `LedgerCols` + 父级下发的同一组 `name_w` / `image_w` / `show_disk` / `show_ports`）；⚠️ 改一行必须同步改另一行。微条与行**共用本列最大值**作基准：行与分组各用各的基准 → 同一列里 3.0% 的分组和 1.6% 的容器都是满格，条长与数值对不上号（微条的意义是「列内可比」） |

计数口径：概览带与筛选 chips 都取**可见容器集合**（实际渲染成行的那些容器），
折叠分组后计数同步变小——三个数字必须同源，否则会出现「台账 3 行但芯片写着全部 9」。

没有 Compose 归属的容器**直接在顶层**（depth=0），不另设「独立容器」分组带。

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
