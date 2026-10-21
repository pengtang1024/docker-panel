# Compose 侧栏树（方向 B）

> 隶属 [architecture.md 总纲](../architecture.md) 与 [ui-interaction.md](ui-interaction.md)。
> 本文是 Compose 侧栏树的权威设计文档：形态、状态、筛选语义、落点清单与验收标准。
>
> 决策来源：用户选定「方向 B —— 左侧导航树」（2026-10-21）。

---

## 1. 目标与非目标

**目标**

- Compose 项目/服务有**层级可见性**：项目 → 服务两级，容器仍留在台账。
- 按项目/服务**快速筛选容器**，替代筛选栏里平铺的 compose chips。
- 侧栏是**导航**（选范围），台账是**数据**（列对齐的密集表）——两者职责不混。

**非目标（本期不做）**

- 不做三级（容器作为树叶子）：容器状态会在侧栏与台账两处重复，易不同步。
- 不做项目级批量启停（点项目节点直接操作整组容器）——另开一期。
- 不做侧栏拖拽调整宽度 / 手动折叠：208px 固定宽。
- 不做 Docker Desktop 的镜像/卷树：侧栏只管 compose。

---

## 2. 形态与布局

### 2.1 位置与尺寸

- 侧栏宽 **208px 固定**，**仅容器视图渲染**（`view == 0`）；切到镜像/网络/卷时侧栏整块消失，
  主区变宽（卡片网格反而多一列）。
- 侧栏位于 TopBar **下方**、与主列表工作区并排；TopBar 与 FootBar 仍通栏。
- 侧栏与主区之间 1px 分隔线（`Theme.line`）。

```
┌──────────────────────────────────────────────┐
│ TopBar（通栏：logo / 搜索 / 主题 刷新 清理 拉取）  │
├──────────────┬───────────────────────────────┤
│ 全部容器  8   │ SubBar（chips / 排序 / 视图 tabs）│
│ ▾ proj-a   3  ├───────────────────────────────┤
│     web    2  │ 台账：概览带 → 表头 → 行…        │
│     db     1  │                               │
│ ▸ proj-b   5  │                               │
│ 独立容器  2   │                               │
├──────────────┴───────────────────────────────┤
│ FootBar（通栏）                                 │
└──────────────────────────────────────────────┘
```

### 2.2 节点模型

| 节点 | 层级 | 展示 | 说明 |
|------|------|------|------|
| 全部容器 | 顶层 | 名称 + 总容器数 | 常驻第一行；「无选中」时高亮，点击 = 清除 compose 范围 |
| 项目 | 0 级 | 展开箭头 + 项目色点 + 名称 + 容器数 | 来自 label `com.docker.compose.project` |
| 服务 | 1 级 | 缩进 + 名称 + 容器数 | 来自 label `com.docker.compose.service` |
| 独立容器 | 顶层 | 名称 + 容器数 | 非 compose 容器；**仅当数量 > 0 时出现** |

- 节点行高 26px（导航元素，比台账行 30px 略紧）。
- 项目色点沿用 `ui_rows::proj_color`（8 色稳定散列），与原 compose chips 同色。
- 容器数右对齐、等宽体、弱化色（`Theme.ink_3`）。
- 树为空（无 compose 容器）时只显示「全部容器」行，不显示空态文案。

### 2.3 展开状态

- 项目节点可展开/收起；**默认全部收起**（首屏保持台账的全貌，不被树吃掉注意力）。
- 展开状态持久化到配置文件，重启恢复。

---

## 3. 数据模型与状态

### 3.1 Slint 模型

`ui/models.slint` 新增（替代 `ComposeRow`）：

```slint
export struct ComposeNode {
    kind: int,        // 0=全部 1=项目 2=服务 3=独立容器
    depth: int,       // 0=项目层 1=服务层
    project: string,  // 所属项目（kind=2/3 时可能为空）
    service: string,  // 服务名（kind=2）
    label: string,    // 展示名
    color: color,     // 项目色
    n: int,           // 容器数（绝对值，不随状态筛选变化）
    expanded: bool,   // 是否展开（仅 kind=1 有意义）
    selected: bool,   // 是否为当前 compose 范围
}
```

struct 内禁止嵌套数组（会失去 Send，无法跨线程投递）——本结构全是标量 ✓。

### 3.2 Rust 状态（`AppState`）

```rust
/// compose 筛选范围（与状态筛选 filter 正交）
pub enum ComposeScope {
    All,                              // 不限
    Project(String),
    Service { project: String, service: String },
    Standalone,                       // 仅非 compose 容器
}

pub compose_scope: ComposeScope,        // 用户操作过的状态 → 回调独占写
pub compose_expanded: BTreeSet<String>, // 展开的项目名
pub compose_epoch: Arc<AtomicU64>,      // 树模型过期判定（对齐 rows_epoch 套路）
```

**写入方纪律（AGENTS.md 规范 #18）**

- `compose_scope` / `compose_expanded` 是**用户操作过的状态**：只由 UI 回调在 UI 线程同步写入
  `AppState` → 立刻重建树模型 + 重建台账行 → 灌 UI。
- consumer **只刷数据**：容器列表变化时按当前 scope/expanded 重建树的**计数**，
  不回写 scope/expanded；灌 UI 前比对 `compose_epoch`，变了就丢弃这批旧树
  （与 `rows_epoch` 同套路，防「锁内建、锁外应用，期间用户改了条件」的竞态）。

### 3.3 持久化（`src/config.rs`）

现有 7 字段手写 JSON，追加 4 个：

| 字段 | 类型 | 说明 |
|------|------|------|
| `compose_kind` | int | 0=All 1=Project 2=Service 3=Standalone |
| `compose_project` | string | 项目名（kind=1/2） |
| `compose_service` | string | 服务名（kind=2） |
| `compose_expanded` | string[] | 展开的项目名数组 |

读取时字段级回落默认值；项目名/服务名走现有 `json_escape`（不引入分隔符拼接，避免转义歧义）。

---

## 4. 筛选语义

**两个维度正交，AND 组合**：

| 维度 | 来源 | 取值 |
|------|------|------|
| 状态筛选 | 筛选栏 chips（`filter`） | all / running / stopped |
| compose 范围 | 侧栏树（`compose_scope`） | All / Project / Service / Standalone |
| 搜索 | 顶栏搜索框（`search_q`） | 关键词 |

生效顺序：状态筛选 → compose 范围 → 搜索。

**筛选栏 chips 改造**：移除 compose chips（`for row[i] in root.compose_rows : Chip`），
只保留 全部 / 运行中 / 已停止 三个。compose 选择只有一个入口（侧栏树），避免两处状态打架。

**两类计数的口径不同（有意为之）**：

| 计数 | 口径 | 理由 |
|------|------|------|
| 树节点计数（项目 / 服务 / 独立容器） | **绝对值**：该组容器总数，不随状态筛选变化 | 回答「这一组有多少」，与 Docker Desktop 侧栏一致 |
| 筛选栏 chips 计数（全部 / 运行中 / 已停止） | **compose 范围内的分布**（`ui_rows::scoped_counts`） | 回答「当前范围内怎么分布」；写全量会让台账 5 行而芯片写着「全部 7」 |

**排序**：默认仍是 Group（project → service → container）。选中某个项目时，
台账内只剩该项目的容器，组内相对顺序不变。

---

## 5. 交互细节

| 操作 | 结果 |
|------|------|
| 点展开箭头（▸/▾） | 展开/收起该项目的服务列表；**不改变筛选范围** |
| 点节点标签 | 选中该范围 → 台账立即按新范围过滤 |
| 再次点已选中的节点 | 取消选中 → 回到「全部容器」 |
| 点「全部容器」 | 清除 compose 范围 |
| 选中项目节点 | **自动展开**该项目（便于继续点服务细分） |
| 切换视图（Ctrl+1~4 / 视图 tabs） | 侧栏随容器视图消失/出现；**compose 范围保留**（树在容器视图里可见，不是隐藏筛选） |

- 命中顺序：展开箭头的 TouchArea 必须声明在标签 TouchArea **之前**（AGENTS.md 规范 #19，
  Slint 按声明顺序命中，后声明的兄弟节点先拿事件）。
- 选中态：整行浅色底 + 左侧 2px 强调条（`Theme.accent`）。
- **收起项目时仍要留记号**：选中的服务藏在收起的项目里时，项目节点保留 2px 强调条
  （`ComposeNode.contains_selected`），否则筛选会凭空消失、用户不知道台账为什么只剩 1 行。
- 侧栏在 `view != 0` 时不实例化 → 条件渲染移除焦点元素后焦点悬空 → 切视图回调里
  主动调 `global-shortcuts.focus()` 归还焦点（与 app.slint Esc 分支同款处置）。

---

## 6. 前置改造：`ui/app.slint` 拆分

`ui/app.slint` 正文已达 **295/300 行**（R2 口径），加侧栏必然超限 → **先拆再接**。

| 动作 | 内容 | 结果 |
|------|------|------|
| 拆出 `ui/main-area.slint` | SubBar + BatchBar? + CardsArea 三件套（主列表工作区），承接数据模型与回调 | app.slint 正文 −7 行（288/300） |
| 侧栏接入 | **侧栏落在 `main-area.slint` 内**（`HorizontalLayout { ComposeTree(208px), 列表列 }`），app.slint 只加属性与回调绑定 | app.slint 正文 295/300 |
| SortMenu 归属 | SortMenu 留在 app.slint（浮层要在最上层），锚点改由 `main-area` 的 `out property menu_x / menu_y` 提供 | 排序菜单定位不变 |

拆分只搬代码、不改行为；`main-area` 用 `<=>` 双向绑定承接原有属性，回调在 app.slint 注入。

> 附带：`src/ui_state.rs` 因接入 compose 字段涨到 307 行（破 300 上限），
> 已把配置读写拆成 `src/ui_state/config_io.rs` 子模块（`AppState ↔ config.json`）。

---

## 7. 落点清单

**新增（已落地）**

| 文件 | 职责 |
|------|------|
| `ui/compose-tree.slint` | 侧栏树组件（节点行 / 展开箭头 / 选中态；@material ListView） |
| `ui/main-area.slint` | 主列表工作区（从 app.slint 拆出）+ 侧栏树的宿主 |
| `src/ui_rows/compose_tree.rs` | 树节点构建（扁平可见节点列表：展开的项目展开出服务） |
| `src/callbacks_compose.rs` | 树节点点击 / 展开收起回调（compose 状态的唯一写入方） |
| `src/compose_scope.rs` | `ComposeScope`（全部 / 项目 / 服务 / 独立容器）+ `ComposeState`（范围 + 展开集合） |
| `src/ui_state/config_io.rs` | AppState ↔ config.json 读写（从 ui_state.rs 拆出，守住 300 行上限） |
| `docs/arch/compose-tree.md` | 本文 |

**修改**

| 文件 | 改动 |
|------|------|
| `ui/app.slint` | 拆出 main-area + 接入侧栏 + 切视图清 scope |
| `ui/models.slint` | `ComposeRow` → `ComposeNode` |
| `ui/sub-bar.slint` | 移除 compose chips 与 `compose_rows` 属性 |
| `src/ui_rows/mod.rs` | `build_compose_rows` → `build_compose_tree` |
| `src/ui_rows/containers.rs` | 筛选链路加 compose scope 判定 |
| `src/ui_state.rs` | `compose_scope` / `compose_expanded` / `compose_epoch` |
| `src/consumer.rs` | 容器变化时重建树模型（带 epoch 判定） |
| `src/config.rs` | 追加 4 个持久化字段 |
| `src/app-actions.slint` | 新增树回调声明 |
| `translations/…/docker-panel.po` | 新文案（Compose / 全部容器 / 独立容器 / 空态） |
| `README.md` / `AGENTS.md` / `docs/architecture.md` / `CHANGELOG.md` | 同步 |

---

## 8. 风险与既有约束

| 风险 | 处置 |
|------|------|
| **台账可用宽度减少** | 响应式隐列阈值按**台账自身宽度**判定（`root.width`），侧栏占掉的宽度自动计入，无需改阈值；实测 1280px 窗口下台账剩 1072px → 端口 / 项目列被隐（用户已确认保留项目列，接受此取舍） |
| **列起点契约（R20）** | 树在台账**外侧**，不参与台账列宽计算 → 台账逐列同 x 同宽不受影响 |
| **AppState 状态回写竞态** | scope/expanded 由回调独占写；consumer 侧用 `compose_epoch` 判定过期（同 `rows_epoch`） |
| **条件渲染焦点悬空** | 切视图后主动 `global-shortcuts.focus()` |
| **TouchArea 命中顺序** | 箭头热区声明在标签热区之前 |
| **Slint struct 不能含嵌套数组** | `ComposeNode` 全标量 |
| **delegate 不能 import AppActions** | 树节点只声明 `callback` 属性，由 app.slint 注入 |
| **chips 计数口径打架** | chips 计数改走 `ui_rows::scoped_counts`（compose 范围内分布），与台账行数同口径 |

---

## 9. 验收标准（已全部通过 MCP 像素验收）

1. 侧栏 208px，仅容器视图出现；切视图时消失/出现，TopBar 与 FootBar 仍通栏。
2. 树结构正确：项目（带色点 + 计数）/ 服务（缩进 + 计数）/ 独立容器（仅 >0 时）。
3. 点箭头只展开收起、不改筛选；点标签改筛选；再点一次取消。
4. 台账随范围正确过滤；筛选栏 chips 只剩 3 个。
5. 台账列起点**逐列不变**（MCP `get_element_tree` 比对 `absolutePosition.x` 与 `size.width`）。
6. 展开状态与范围持久化，重启恢复。
7. 门禁全绿：`cargo fmt --check` / `clippy -D warnings` / `cargo test`（29 passed）/ `cargo build --release --locked`；
   旧项目名零残留；所有新增/改动文件正文 ≤300 行。
