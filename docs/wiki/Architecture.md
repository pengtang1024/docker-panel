# 架构设计

本文介绍 dvkit 的模块分层、线程模型、消息流与关键设计决策。
仓库中的同名文档位于 `docs/architecture.md`，内容保持同步。

## 1. 技术栈

| 组件 | 选型 | 用途 |
|------|------|------|
| GUI | egui / eframe 0.36 + egui_extras | 即时模式原生 GUI、表格 |
| Docker 客户端 | bollard 0.21 | Docker Engine API |
| 异步运行时 | tokio（多线程） | 后台 Docker 调用 |
| 流式 | futures | 日志流、拉取进度、并发采样 |

整体是经典的**即时模式 GUI + 后台异步任务 + channel 消息**结构，
UI 线程零阻塞。

## 2. 模块分层

```
main.rs                         入口：创建 eframe 原生窗口（1000x700，最小 820x520）
└── app/                        GUI 层：状态、渲染、帧调度
    ├── mod.rs                    DvApp 全局状态、Tab 调度、弹窗装配
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
    ├── containers.rs             列表、资源采样、启停重启删除
    ├── images.rs                 列表（含关联容器统计）、拉取、删除
    └── system.rs                 prune 清理、follow 日志流
├── models.rs                   纯数据结构：ContainerInfo / ImageInfo / Msg / …
├── utils/                      主题色板、字节/时间格式化、CJK + 图标字体加载
└── widgets/                    无状态 UI 组件：按钮、表头、选择框、折线图
```

依赖单向：`main → app → docker → models`；`utils` 与 `widgets` 是底层
设施，被各层复用，不反向依赖业务模块。

**代码规范：单文件超过 300 行必须按职责拆分。**

## 3. 线程模型与消息流

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
                 │  list / sample / pull / start/stop/… /    │
                 │  follow_logs（长驻日志流，独立 channel）    │
                 └─────────────────────────────────────────┘
```

每帧固定顺序：

1. `pump_messages()` 非阻塞排空消息队列；
2. `poll_logs()` 排空日志流事件；
3. `maybe_refresh()` 按节奏发起后台任务；
4. 渲染顶栏与当前 Tab；
5. 装配六类弹窗（日志、监控、清理、确认、错误、结果）；
6. `request_repaint_after(250ms)`，保证后台消息最迟 250ms 内被处理。

设计要点：

- 所有 Docker 调用都是 async，只在 tokio 后台任务执行，UI 永不阻塞。
- 日志流使用独立 channel 与 `JoinHandle`；关窗时 `abort()` 中断任务，
  channel 断开后 `follow_logs` 自行退出。
- 拉取进度通过闭包回调转成 `Msg::PullProgress`，最多保留 200 条。
- 错误经 `last_error` 去重，同一错误连续出现只弹一次。

## 4. 刷新节奏

| 数据 | 周期 | 触发与限流 |
|------|------|------------|
| 容器列表 | 2s | `containers_loading == false` 时才发起 |
| 镜像列表 | 15s | 仅停留在镜像 Tab |
| 资源采样 | 2s | `stats_pending == 0` 才发起；`buffer_unordered(4)` 并发上限 4 |
| 日志 | 实时 | follow 流，`poll_logs()` 逐帧排空 |
| 手动刷新 | — | 工具条按钮、筛选切换立即触发 |

## 5. 关键 UI 设计

### 5.1 TableOutcome：规避借用冲突

egui 表格渲染闭包内持有 `&self` 不可变借用，不能同时改状态。
表格组件因此只收集交互结果，渲染完后统一应用：

```rust
struct TableOutcome<C> {
    select_all: Option<bool>,   // 表头全选框
    sort_click: Option<C>,      // 被点击的排序表头
    toggles: Vec<String>,       // 单击/复选框切换的行 key
    actions: Vec<RowAction>,    // 行内按钮/右键菜单动作
}
```

工具条上的批量按钮同理：闭包内置局部 flag，闭包外执行。

### 5.2 选择模型

- 容器选择集 `HashSet<容器 ID>`；镜像选择集 `HashSet<"名称:tag">`。
- 批量对象 = **选择集 ∩ 当前可见行**（搜索过滤后），防止误操作。
- 刷新后按新列表剪除已消失项，不留悬空选中。
- 点行内控件不触发整行选中切换。

### 5.3 排序稳定性

- 同列再点翻转升降序，换列默认升序；
- 所有排序以 ID 兜底（`then_with`），采样数据变化不会让行乱跳；
- 状态列按权重排（running 最前，exited/dead 最后）。

### 5.4 Compose 分组

`build_disp()` 产出过滤排序后的行 + `Vec<DispRow>` 展示序列：
带 `com.docker.compose.project` label 的容器按项目名收进组头，
散容器在前；组头整行手绘（强调条 + 折叠三角 + 项目名 + 成员数），
折叠状态记录在 `collapsed_groups`。

### 5.5 无感刷新

容器列表刷新时，按 ID 把上一次的六个采样字段
（CPU / 内存用量 / 内存上限 / 磁盘读写 / 网络收发）拷到新记录，
避免指标每 2 秒闪归零。loading 转圈只在首屏显示，日常刷新不抖动布局。

### 5.6 资源监控小图

自绘 110px 折线图：网格线 + 曲线 + 逐段矩形面积填充 + 当前值标注。
CPU 上限自适应（历史峰值 ×1.2，下限 10%），内存固定 0–100%。
历史保留 120 点 ≈ 4 分钟，`VecDeque` 头部淘汰。

## 6. Docker 接入约定

- 统一 `Docker::connect_with_local_defaults()`：自动读 `DOCKER_HOST`，
  默认本机 unix socket；每次调用新建连接，不跨任务持有状态。
- 错误统一转中文 `String` 上抛，由 UI 弹窗展示。
- 删除容器 `force=true`；镜像删除被容器引用时由 Docker 拒绝。
- 清理：`prune_containers` / `prune_images(dangling=true|false)`；
  勾选「未使用镜像」时不再单独清 dangling（前者已包含）。
- 镜像关联容器数：优先按 image_id 精确匹配，匹配不到按 repo tag 兜底。
- Compose 归属通过 `com.docker.compose.project` /
  `com.docker.compose.service` 两个 label 识别。

## 7. 资源指标计算

与 `docker stats` 对齐：

- **CPU%**：`(cpu_total - precpu_total) / (system - presystem) × online_cpus × 100`，
  `stream=false` 单次采样；
- **内存**：usage 扣除缓存文件（cgroup v2 `inactive_file`，
  v1 `total_inactive_file`）；
- **磁盘 IO**：`blkio_stats.io_service_bytes_recursive` 按 Read/Write
  累加；cgroup v2 无明细，显示 0；
- **网络 IO**：汇总所有网卡 `rx_bytes / tx_bytes`。

## 8. 主题与字体

- GitHub Dark 风格暗色主题：四级背景层次、语义色常量集中在
  `utils/mod.rs`、柔和投影、浮动滚动条、五档字号。
- 中文字体：递归扫描四个系统字体目录，按优先级挑选 CJK 字体，
  同族 Regular 优先，追加到字体家族末尾不覆盖默认字体。
- Material Icons 以 `include_bytes!` 编进二进制放 fallback 位，
  码位常量集中在 `widgets::ic`。
- 找不到中文字体仅 stderr 警告，程序降级可用。

## 9. 消息类型

| 变体 | 触发 | UI 处理 |
|------|------|---------|
| `Containers(Result<Vec<ContainerInfo>>)` | 列表刷新 | 保留采样字段、更新角标、剪除失效选择 |
| `Images(Result<Vec<ImageInfo>>)` | 镜像刷新 | 替换列表、剪除失效选择 |
| `Sample(id, Result<ResourceSample>)` | 并发采样 | 更新表格行 + 追加曲线历史 |
| `PullProgress(String)` | 拉取回调 | 追加进度（上限 200 条） |
| `PullDone(Result<String>)` | 拉取结束 | 清空输入、弹窗、刷新镜像列表 |
| `ActionDone(desc, Result<String>)` | 单个/批量操作 | 成功刷新两个列表；失败弹聚合错误 |
| `CleanupDone(Result<String>)` | 清理 | 结果弹窗 + 刷新 |
