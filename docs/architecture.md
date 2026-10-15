# Docker Panel 架构设计

本文档描述 Docker Panel 的模块分层、线程模型、消息流与关键设计决策。

## 1. 分层与模块职责

```
main.rs                         入口：创建 eframe 原生窗口（1000x700，最小 820x520）
└── app/                        GUI 层：状态、渲染、帧调度
    ├── mod.rs                    DvApp 全局状态、Tab 调度、弹窗装配
    ├── constants.rs              策略常量：刷新/采样周期、历史容量、窗口稳定 ID
    ├── columns.rs                列枚举（CtlCol/ImgCol）、宽度表、排序比较器、显示列菜单
    ├── containers/               容器 Tab
    │   ├── mod.rs                  工具条、批量操作条、交互结果应用
    │   ├── header.rs               顶部品牌头：Logo + Tab + Docker 连接角标
    │   ├── prep.rs                 build_disp：搜索过滤 + 排序 + compose 分组序列
    │   ├── table.rs                表格骨架：表头排序/全选 + 横向滚动
    │   └── row.rs                  行渲染：组头、单元格、右键菜单、双击/单击
    ├── images/                   镜像 Tab
    │   ├── mod.rs                  工具条、拉取输入、过滤排序、批量删除
    │   └── table.rs                表格：表头、单元格、右键菜单
    ├── tasks/                    后台任务层
    │   ├── mod.rs                  任务发起：刷新/采样/单个与批量操作/拉取/日志流
    │   └── messages.rs             消息泵 pump_messages、poll_logs、maybe_refresh
    └── windows/                  弹窗层
        ├── mod.rs                  日志窗口、资源监控窗口 + 通用弹窗小部件
        ├── confirm.rs              删除二次确认（单个/批量，带预览清单）
        └── dialogs.rs              清理、错误、执行结果弹窗
├── docker/                     Docker API 层（全部 async）
    ├── mod.rs                    connect()：本机默认连接
    ├── containers.rs             列表、资源采样、启停重启删除
    ├── images.rs                 列表（含关联容器统计）、拉取、删除
    └── system.rs                 prune 清理、follow 日志流
├── models.rs                   纯数据结构：ContainerInfo / ImageInfo / Msg / …
├── utils/                      基础设施
    ├── mod.rs                    GitHub Dark 主题色板、字节/时间格式化、状态文案
    └── fonts.rs                  系统 CJK 字体扫描注入 + Material Icons
└── widgets/                    无状态 UI 组件（自由函数，不持有业务状态）
    ├── ic.rs                     Material Icons 码位常量
    ├── control.rs                图标按钮、选择框、菜单项、搜索框、徽章
    ├── table.rs                  排序表头、折叠三角、右对齐文本
    └── chart.rs                  CPU/内存折线小图
```

依赖单向：`main → app → docker → models`；`utils` 与 `widgets` 是底层设施，
被各层复用，不反向依赖业务模块。`app` 内部子模块通过 `use super::*` 共享
`DvApp` 与类型定义，每个文件只承担一个明确职责。

> 代码规范：单文件超过 300 行必须按职责拆分。当前最大的文件也在此线以内。

## 2. 线程模型与消息流

```
                 ┌─────────────────────────────────────────┐
                 │              UI 线程 (eframe)            │
                 │  pump_messages()  poll_logs()            │
                 │  maybe_refresh()  → 渲染 → 弹窗          │
                 └───────┬──────────────────────▲───────────┘
              spawn 任务 │                      │ std::mpsc::Sender<Msg>
                         ▼                      │
                 ┌─────────────────────────────────────────┐
                 │        tokio 多线程运行时 (DvApp.rt)      │
                 │  list_containers / list_images /         │
                 │  sample_container / pull_image /          │
                 │  start/stop/restart/remove / cleanup      │
                 │  follow_logs（长驻日志流，独立 channel）    │
                 └─────────────────────────────────────────┘
```

每帧固定顺序（`DvApp::ui`）：

1. `pump_messages()`：`try_recv` 非阻塞排空消息队列；
2. `poll_logs()`：排空日志流事件；
3. `maybe_refresh()`：按节奏发起后台任务；
4. 渲染顶栏与当前 Tab；
5. 依次装配日志、监控、清理、确认、错误、结果六类弹窗；
6. `ctx.request_repaint_after(250ms)`，保证后台消息最迟 250ms 内被处理。

关键点：

- **UI 永不阻塞**：`docker/` 的所有函数都是 async，只在 tokio 后台任务执行；
  结果打包为 `Msg` 枚举经 `std::mpsc` 回传。
- **日志流独立通道**：日志窗口持有自己的 `Sender<LogEvent>` 与 `JoinHandle`，
  关窗时 `abort()` 中断长驻任务；channel 断开后 `follow_logs` 自行退出。
- **拉取进度回调**：`pull_image(spec, on_progress)` 用闭包把进度文本转成
  `Msg::PullProgress`，进度最多保留 200 条（超出一次裁掉前 100 条）。
- **错误去重**：`show_error()` 配合 `last_error`，同一错误连续出现只弹一次；
  弹窗关闭后清空记录，之后再次出现仍会提示。

## 3. 刷新节奏

| 数据 | 周期 | 触发与限流 |
|------|------|------------|
| 容器列表 | 2s | `containers_loading == false` 时才发起 |
| 镜像列表 | 15s | 仅停留在镜像 Tab；非 loading 时发起 |
| 资源采样 | 2s | `stats_pending == 0` 才发起，天然限流；并发上限 4 |
| 日志 | 实时 | follow 流，事件在 `poll_logs()` 中排空 |
| 手动刷新 | — | 工具条刷新按钮、筛选切换时立即触发 |

采样任务用 `futures::stream::iter(...).buffer_unordered(4)` 并发，
所有结果在同一个后台任务里收集后逐条回传。监控窗口打开的容器即使
不在列表（如刚停止）也会被临时加入采样名单。

## 4. UI 层关键设计

### 4.1 TableOutcome：规避 egui 借用冲突

表格渲染闭包内持有 `&self` 不可变借用，不能同时修改状态。因此表格组件
不直接改状态，而是把交互结果收集进 `TableOutcome`：

```rust
struct TableOutcome<C> {
    select_all: Option<bool>,   // 表头全选框
    sort_click: Option<C>,      // 被点击的排序表头
    toggles: Vec<String>,       // 单击/复选框切换的行 key
    actions: Vec<RowAction>,    // 行内按钮/右键菜单产生的动作
}
```

渲染结束后由 `ui_containers` / `ui_images` 统一应用：先处理排序与选择，
再逐个 match 执行 `RowAction`（打开窗口或 spawn Docker 操作）。
工具条上的批量按钮同理：闭包内只置局部 flag，闭包外执行。

### 4.2 选择模型

- 容器选择集是 `HashSet<容器 ID>`；镜像选择集是 `HashSet<"名称:tag">`
  （`ImageInfo::key()`），与 `ImageInfo` 的唯一键保持一致。
- 批量操作的对象 = **选择集 ∩ 当前可见行**（经过搜索过滤后），避免误操作
  被关键词藏起来的行。
- 列表刷新后，按新列表剪除已消失项的选择，不会保留悬空选中。
- 点行空白处切换选中；点行内控件（按钮、复选框）不触发整行切换。

### 4.3 排序与行序稳定

- 点表头：同列再次点击翻转升降序；换列默认升序。
- 任何排序都以 ID 做最终兜底键（`then_with`），保证资源采样导致的数据
  变化不会让相同键值的行来回跳动。
- 状态列不按字符串排，而是按 `state_rank` 权重：running → restarting →
  paused → created → removing → exited → dead。

### 4.4 过滤与 Compose 分组（build_disp）

`prep.rs::build_disp()` 产出三样东西：过滤排序后的 `rows`、供表格按序
渲染的 `disp: Vec<DispRow>`、小写搜索词（用于空态文案）。

- 搜索在本地对已拉取数据做即时过滤，不发 Docker 请求。
- 开启 Compose 分组后，带 `com.docker.compose.project` label 的容器按
  项目名（`BTreeMap` 保证字母序）收进组头 `DispRow::Group { project, count }`，
  不带 label 的散容器排在前面。
- 组头整行手绘（强调条 + 折叠三角 + 项目名 + 成员数），点击切换折叠；
  折叠状态记录在 `collapsed_groups: HashSet<String>`。

### 4.5 无感刷新与指标保留

处理 `Msg::Containers(Ok(list))` 时，先按容器 ID 建立旧数据哈希表，
把 `cpu_percent / mem_usage / mem_limit / block_* / net_*` 六个采样字段
从旧记录拷到新记录，避免 2 秒刷新瞬间指标闪归零。loading 转圈只在
首屏（列表为空）显示，日常刷新不让工具条布局抖动。

### 4.6 弹窗状态机

| 状态 | 类型 | 用途 |
|------|------|------|
| `confirm` | `Option<Confirm>` | 单个/批量删除容器、单个/批量删除镜像；批量带最多 8 项预览清单 |
| `error` | `Option<String>` + `last_error` | Docker API 错误，多行聚合文本等宽滚动展示 |
| `result` | `Option<String>` | 操作/拉取/清理成功结果 |
| `cleanup_open` + 三个勾选 | — | 清理弹窗，未勾任何项时确认按钮禁用 |
| `log_view` | `Option<LogView>` | 日志窗口（同屏最多一个） |
| `resource_view` | `Option<ResourceView>` | 资源监控窗口 |

弹窗统一锚定屏幕中心；确认窗不可折叠、不可缩放，防止误操作时丢失焦点。

### 4.7 资源监控小图

`widgets::chart::draw_chart()` 是 110px 高的自绘折线图：深底 + 3 条
水平网格线 + 曲线 + 逐段矩形面积填充（避免非凸多边形接缝问题）+
右上角当前值。CPU 曲线自适应上限（历史峰值 ×1.2，下限 10%），
内存曲线固定 0–100%（按容器内存上限换算百分比）。历史保留
120 个采样点 ≈ 4 分钟（`VecDeque` 头部淘汰）。

## 5. Docker 接入约定

- 连接统一走 `docker::connect()` → `Docker::connect_with_local_defaults()`：
  自动读 `DOCKER_HOST`，默认本机 unix socket；Windows/macOS 走各自默认管道。
- 每次 API 调用新建连接（bollard 连接轻量，且避免跨任务持有状态）。
- 错误统一转中文 `String`（如 `"获取容器列表失败: {e}"`）上抛，UI 层弹窗展示。
- 删除容器 `force=true`，运行中容器也能删；删除镜像同样 `force=true`，
  但被容器引用时 Docker 仍会拒绝，错误直接展示给用户。
- 清理：停止容器用 `prune_containers`；悬空镜像用 `dangling=true` 过滤；
  未使用镜像用 `dangling=false`，等价 `docker image prune -a`
  （勾选「未使用」时不再单独清理 dangling，因为前者已包含）。
- 镜像列表关联容器数：优先按 image_id 精确匹配，匹配不到再按
  `repo_tag == 容器镜像名` 兜底求和。
- Compose 归属通过 label 识别：`com.docker.compose.project` /
  `com.docker.compose.service`。

## 6. 资源指标计算

- **CPU%**：`(cpu_total - precpu_total) / (system - presystem) × online_cpus × 100`，
  与 `docker stats` 公式一致；用 `stream=false` 取单次采样；分母无效时为 0。
- **内存**：`usage` 扣除缓存文件（cgroup v2 取 `inactive_file`，
  v1 取 `total_inactive_file`）。
- **磁盘 IO**：读取 `blkio_stats.io_service_bytes_recursive` 按
  Read/Write 累加；cgroup v2 下无此明细，显示 0（与 docker stats 一致）。
- **网络 IO**：汇总所有网卡的 `rx_bytes / tx_bytes`。

## 7. 主题、字体与图标

- 主题在 `utils::apply_theme()` 一次性配置：四级背景层次
  （DEEP_BG 顶栏/日志 → PANEL_BG 面板 → SURFACE_BG 表头/输入框 →
  斑马纹）、控件三态配色、柔和投影、浮动滚动条、五档字号。
- 语义色集中在 `utils/mod.rs` 常量：蓝（主操作）/绿（运行、启动）/
  红（停止、删除、错误）/琥珀（重启、警告）/青（监控、compose）。
- egui 默认字体不含 CJK。`load_cjk_fonts()` 递归扫描
  `/usr/share/fonts`、`/usr/local/share/fonts`、`~/.local/share/fonts`、
  `~/.fonts`，按「思源黑体 → Noto Sans/Serif CJK → 文泉驿 →
  Droid Sans Fallback → Uming」优先级挑选，同族内 Regular 字重优先，
  找到后追加到 Proportional / Monospace 家族**末尾**，不覆盖默认字体。
- Material Icons 字体（`assets/MaterialIcons-Regular.ttf`）以
  `include_bytes!` 编进二进制，同样放在家族末尾作 fallback；
  码位常量集中在 `widgets::ic`，界面里不出现裸码位。
- 找不到 CJK 字体时仅向 stderr 打警告，程序降级运行（图标与拉丁字符正常）。

## 8. 消息类型一览（models::Msg）

| 变体 | 触发任务 | UI 处理 |
|------|----------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | 列表刷新 | 保留采样字段、更新状态角标、剪除失效选择 |
| `Images(Result<Vec<ImageInfo>>)` | 镜像刷新 | 替换列表、剪除失效选择 |
| `Sample(id, Result<ResourceSample>)` | 并发采样 | 更新表格行 + 追加监控曲线历史 |
| `PullProgress(String)` | 拉取流回调 | 追加进度文本（上限 200 条） |
| `PullDone(Result<String>)` | 拉取结束 | 成功则清空输入、弹窗并刷新镜像列表 |
| `ActionDone(desc, Result<String>)` | 单个/批量操作 | 成功刷新容器与镜像列表；失败弹聚合错误 |
| `CleanupDone(Result<String>)` | 清理 | 结果弹窗 + 刷新两个列表 |
