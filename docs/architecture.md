# 架构设计（总纲）

> **一句话**：本文件是 docker-panel 的架构入口——技术栈精确版本、模块地图、依赖方向与文件规范；具体实现细节分散在 5 份主题文档里，本页负责告诉你**该读哪一份**。
>
> **适用范围**：改动代码、review PR、排查「为什么这样设计」之前先读本页。

**对应源码**：`src/main.rs`、`src/consumer.rs`、`ui/app.slint`｜**主题文档**：见 §1

写代码时必须按精确版本的 API 写，禁止凭旧版本记忆。版本特有的 API 限制清单：Slint 见仓库根 `AGENTS.md`，bollard 见 [arch/docker-bollard.md](arch/docker-bollard.md)。

## 1. 主题文档导航

本页是总纲，具体主题各有唯一权威源。**改代码前先确认你要改的部分归哪一份管**：

| 文档 | 权威范围 | 什么时候读 |
|------|----------|------------|
| **本文件**（总纲） | 技术栈精确版本、模块地图与依赖方向、文件规范 | 第一次进仓库、不确定该看哪份 |
| [arch/threading.md](arch/threading.md) | 线程模型、消息流（`Msg` 变体 + 流水线）、Rust 回调绑定铁律、Rust 拆分原则 | 动 consumer / tokio 任务 / `Msg` / 锁 |
| [arch/ui-interaction.md](arch/ui-interaction.md) | Slint UI 模块交互（交互总图 + 4 种交互模式 + 文件清单）、Slint 拆分原则 | 动 `ui/*.slint`、加交互或弹窗 |
| [arch/list-layout.md](arch/list-layout.md) | 统一列表布局：四视图列表形态、台账内联 Compose 树、三个资源列表的表头排序、落点清单、验收标准 | 动台账 / 列宽 / 排序 / 分组带 |
| [arch/docker-bollard.md](arch/docker-bollard.md) | Docker 接入约定、bollard 0.21 易踩坑速查（**唯一权威源**） | 调 Docker API、怀疑类型不对 |
| [arch/update-manifest.md](arch/update-manifest.md) | 自动更新的静态清单（`version.json`）与静态 CDN 分发：字段可信度、CI 生成顺序、三级回退链、失败降级、安全边界（**唯一权威源**） | 动更新检查 / 下载 / 校验 |

> [!IMPORTANT]
> 主题文档与总纲冲突时，**主题文档优先**（它是该主题的唯一权威源）；主题文档之间冲突时按 `docs/agents-rules.md` R3「架构优先」裁决，并在该文件记录冲突。

## 2. 技术栈（精确版本）

| 组件 | 精确版本 | Feature / 备注 |
|------|----------|----------------|
| Rust | stable（MSRV **1.99**） | edition 2021；`rust-toolchain.toml` 固定 stable 通道（rustfmt / clippy），`Cargo.toml` 声明 `rust-version = "1.99"`，CI 固定 `1.99.1` |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std`；strict 模式已验证编译通过——详见 AGENTS.md「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency；`compile_with_config` + `with_bundled_translations("translations")` |
| bollard | **0.21** | Docker Engine API 纯 async 客户端；`LogOutput` 是 struct 不是 tuple——详见 [arch/docker-bollard.md](arch/docker-bollard.md) |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false）；必须多线程 runtime，不能 `current_thread`；spawn 前必须 `handle().clone()`——详见 AGENTS.md「tokio 1.x 版本关键用法」 |
| futures / futures-util | **0.3** | `futures-util` 提供 `StreamExt`（`.next().await`），需显式引入 |
| sha2 | **0.10** | 自动更新包 SHA-256 校验；HTTP 走系统 `curl`（不引入 TLS crate） |
| 渲染 | — | 纯软件渲染器（`renderer-software`），**无 Vulkan / GL / WGPU 依赖**，deb 包不依赖 `libgl1` |
| 构建优化 | — | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip（release profile） |

## 3. 模块地图与文件交互

三层结构，**依赖严格单向**（上层可依赖下层，下层一律不反依赖）：

| 层 | 位置 | 职责 | 允许依赖 |
|----|------|------|----------|
| **组装层** | `src/main.rs`、`slint_gen.rs`、`callbacks*.rs` | CLI/单实例/i18n → `AppState` → `MainWindow` → 启动后台任务 → `ui.run()` | 以下全部 |
| **编排层** | `consumer.rs`、`ui_update/`、`ui_apply.rs`、`ui_rows/`、`bootstrap.rs`、`event_watch.rs`、`stale.rs` | 消息泵与节流、变化区域聚合、行构建、后台采样与事件订阅 | 状态层 + Docker 层 |
| **状态层** | `ui_state*`、`models*`、`config.rs`、`render.rs`、`follow_runtime.rs`、`detail_fmt/`、`batch.rs` | `AppState` 与纯数据结构、持久化、日志流运行时、详情格式化 | Docker 层 |
| **接入层** | `docker/*`、`update/*` | bollard 调用、裸 HTTP、更新检查与安装——**纯 async / 纯逻辑，不反依赖任何上层** | 无（仅第三方 crate） |
| **声明层** | `ui/*.slint` | Slint 声明：`app.slint` 是唯一组装入口 | 见 §4 |

<details>
<summary><b>逐文件职责（点击展开）</b></summary>

```text
docker-panel/
├── Cargo.toml                依赖：slint 1.18.1 / bollard 0.21 / tokio / futures / sha2；workspace 成员 `launcher`
├── rust-toolchain.toml       stable 通道 + rustfmt / clippy（本地统一；CI 另钉 1.99.1）
├── build.rs                  slint_build::compile_with_config + with_bundled_translations("translations")
│
├── src/                       Rust 层（所有 ≤ 300 行）
│   ├── main.rs                组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs           include_modules!() 集中入口
│   ├── i18n.rs                locale 检测（zh*→zh_CN）+ Rust 侧 t()/trf()/trfp()（复数，对齐 @tr 的 `"sing" | "plural" % n`）+ slint_language()
│   ├── config.rs              AppConfig 持久化（directories + 手写 JSON 原子写）
│   │
│   ├── consumer.rs            consumer 线程：动态 recv_timeout(≤100ms) + 分批 drain（≤2048）+ 120ms UI 节流（列表刷新由 event_watch 事件驱动）
│   ├── event_watch.rs         Docker `/events` 订阅：噪声黑名单过滤 + 400ms 双沿合并 + 断流退避重连（列表即时性来源，空闲零请求）
│   ├── bootstrap.rs           6s stats 采样 interval（整轮一次投递） + 30s 磁盘占用采样（容器可写层 + 卷占用，`/system/df`） + 首屏四列表加载 + inspect 首容器 + 守护进程版本查询（独立任务）+ 主题监听
│   ├── stale.rs               采样过期判定：连续多轮无样本 → 边沿触发状态位（stats 3 轮 ≈18s / df 2 轮 ≈60s）
│   │
│   ├── models.rs              纯数据结构：Msg / ContainerInfo / ImageInfo / ContainerDetail / NetworkInfo / VolumeInfo / ImageDetail / ResourceSort
│   ├── models/update.rs       自动更新类型：UpdateInfo / InstallPlan / UnsupportedReason（models 层）
│   ├── batch.rs               BatchState 多选状态（purge_dead）
│   │
│   ├── ui_state.rs            AppState（Arc<Mutex<>> 可共享状态）+ 刷新去重/超时 + 日志流状态字段 + cleanup_counts + persist
│   ├── ui_state/log_sync.rs   日志窗口裁剪 + UI 行模型同步账（LogSyncPlan / trim_logs / log_sync_plan / commit_log_sync）
│   ├── ui_state/config_io.rs  AppState ↔ config.json 读写（字段搬运，越界值回落默认；从 ui_state 拆出）
│   ├── follow_runtime.rs      日志流运行时：建流（单流 / 分组合并多流共享取消信号）/退避重连/探针判定/保活降级/结局分类（从 ui_state 拆出）
│   ├── log_status.rs          日志流状态提示文案唯一定义处（4 条 i18n key：重连中 / 已恢复 / 已停止 / 洪峰抽样）
│   ├── render.rs              CachedModel 差量同步（sync_model：数据 PartialEq 相同则跳过 set_vec）+ 日志行模型 Append/Reset
│   ├── detail_fmt/            详情格式化：mod（公共 head/kv/format_bytes/network_counts）+ container / image / netvol
│   ├── ui_rows/               行构建：mod（hit_any 搜索命中）+ containers（容器台账行）/ ledger（概览带 + 台账入口）/ compose_tree（台账内联树行 + tests.rs）/ group_totals（分组资源汇总）/ images / netvol（网络/卷，含排序）/ sort（三个资源列表共用的「过滤→排序→映射」骨架）
│   ├── ui_update.rs           DrainResult + 有上限 drain_messages + Msg → 变化区域聚合（日志洪峰防护）
│   ├── ui_update/drain.rs     DrainResult：UiFlag 位掩码 + 声明宏生成 has_ui_changes / logs_only / merge
│   ├── ui_update/update.rs    自动更新消息 → AppState / DrainResult（从 ui_update 拆出）
│   ├── ui_apply.rs            apply_drawer / refresh_lists_now（搜索排序即时重建）+ clear_log_panel / clear_log_status / status_toast
│   │
│   ├── callbacks.rs           回调绑定中枢 + restore_config（含启动焦点引导 boot_tick）
│   ├── callbacks_container.rs 容器启停/重启/删除/logs/inspect（AppActions global）
│   ├── callbacks_drawer.rs    抽屉 Tab 切换（切到日志 Tab 触发 ensure_follow）
│   ├── callbacks_image.rs     镜像删除 / 选中（AppActions global）
│   ├── callbacks_netvol.rs    网络/卷选中与删除（AppActions global）
│   ├── callbacks_compose.rs   台账内联树回调（分组展开 / 收起 + 组操作：启动/停止/重启/删除/合并日志；compose_collapsed 的唯一写入方）
│   ├── callbacks_sort.rs      三个资源列表的表头排序回调（各自独立状态 + 持久化）
│   ├── callbacks_update.rs    自动更新回调：检查 / 安装 / 打开下载页 / 重启 / 三个配置开关
│   │
│   ├── update/
│   │   ├── mod.rs             任务编排 + 启动调度（1h 节流）+ Msg 投递 + UI 视图快照
│   │   ├── discover.rs        更新发现三级回退：清单两级 → API 兜底（fetcher 可注入 + 单测）
│   │   ├── manifest.rs        清单解析 / 资产 URL 派生 / 与 checksums.txt 一致性断言（纯逻辑 + 单测）
│   │   ├── github.rs          GitHub Releases API（第 3 级兜底；系统 curl + JSON 解析，阻塞）
│   │   ├── version.rs         版本比较 / 资产选择 / checksums 解析（纯逻辑 + 单测）
│   │   ├── install.rs         环境探测 / 安装方式判定 / 验签 / 原子安装与自替换（deb 的 pkexec 仅兜底）
│   │   ├── install/managed.rs  启动器布局：versions/<v>/ + current 软链（零 root 安装）
│   │   └── install/download.rs 缓存目录 / curl 下载 / SHA-256 校验
│   │
│   └── docker/
│       ├── mod.rs             connect() + with_timeout（单次调用 10s 上限）+ pub use 重导出
│       ├── containers.rs      list / start / stop / restart / remove
│       ├── images.rs          list / pull（含进度闭包）/ remove
│       ├── logs.rs            follow_logs（Stream → 行批 tx.send，含空闲超时 + 复用定时器）
│       ├── log_batch.rs       行批投递：前缀拼接 / 正常段（每批解析最后一行推游标）/ 回放段投递
│       ├── log_ts.rs          时间戳游标：RFC3339Nano 解析 / 剥前缀 / 纳秒过滤 / 回放缓冲（纯逻辑）
│       ├── log_types.rs       日志流结束原因 / 错误分类 / 退避常量（纯类型，单测不依赖 Docker）
│       ├── stats.rs           fetch_stats + compute_cpu_delta（双采样 CPU%）
│       ├── netvol.rs          list_networks + list_volumes + volume_sizes（裸 HTTP 读 Volumes 键）
│       ├── raw_http.rs        裸 HTTP 请求（Unix socket + chunked 解码；bollard 未建模的接口）
│       ├── image_details.rs   inspect_image → ImageDetail
│       ├── container_detail.rs inspect_container + probe_container（存活性探针）
│       └── system.rs          cleanup prune + version（`/version` 守护进程版本，取不到由上层显示 `—`）
│
├── launcher/                  deb 内的 /usr/bin/docker-panel 验签启动器（独立 workspace 成员）
│   ├── src/main.rs            候选（用户副本 / deb 种子）+ 版本排序 + --print-target
│   ├── src/verify.rs          用户副本验签（checksums.txt + .sig + 运行体 sha256）/ 种子按 root 属主信任
│   ├── src/exec.rs            execveat(O_PATH fd)：校验与执行同一 inode
│   └── src/version.rs         版本解析与比较
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po   gettext 翻译（编译期嵌入）
│
└── ui/                        Slint 层（正文均 ≤ 300 行）
    ├── app.slint              MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 布局组装 + M3 色板接线
    ├── overlays.slint         窗口级浮层：排序菜单 / 卡片与分组菜单 / 弹窗 / 全局 TipLayer（声明在内容之后 → 最上层）
    ├── app-actions.slint      AppActions global：UI → Rust 的唯一回调入口
    ├── theme.slint            浅/深双套色板（31 个颜色令牌）+ theme_mode 三态（is_dark 为 out）
    ├── material-theme.slint   M3 色板映射：Theme.* → MaterialScheme（49 角色全覆盖）
    ├── models.slint           ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair / LogRow / LedgerSummary struct
    ├── top-bar.slint          TopBar：logo + 连接状态 + 搜索框 + 主题/刷新/清理/更新/拉取按钮
    ├── sub-bar.slint          SubBar：筛选 chips + 排序 + 4 视图 tabs（chips/排序/多选仅容器视图）
    ├── list-area.slint        资源列表区：容器视图走台账，镜像/网络/卷走表格列表（含空状态 + 背景点击层）
    ├── main-area.slint        主列表工作区：SubBar + BatchBar + ListArea 组装（视图切换的唯一入口）
    ├── ledger.slint           容器台账区：列宽统一计算 + 表头（可点击排序）+ @material ListView 虚拟化行
    ├── ledger-row.slint       台账行 delegate（30px）：状态轨 / 勾选框 / 微条 / 行内操作
    ├── ledger-tree-row.slint  台账内联树分组带 delegate（项目带/服务带：箭头 + 色点 + 计数 + 组内资源汇总 + 组操作按钮，通栏）
    ├── ledger-band.slint      台账概览带：4 个计数 + CPU 占用 TOP4
    ├── ledger-cols.slint      台账列宽契约 global（表头与行共用同一组列宽）
    ├── image-list.slint       镜像表格列表（inherits TableShell）：repo:tag · 大小 · 创建时间 + 行内 play/more
    ├── network-list.slint     网络表格列表（inherits TableShell）：名称 · driver · 子网
    ├── volume-list.slint      卷表格列表（inherits TableShell）：名称 · 大小 · 挂载点
    ├── components-list.slint  列表共用基件：ListCols 列宽契约 + ListHeadCell 表头单元格 + TableShell 表格外壳
    ├── skel.slint             骨架屏占位
    ├── foot-bar.slint         底部 9 项资源概览（含镜像大小）
    ├── batch-bar.slint        批量操作浮动栏
    ├── drawer.slint           右侧抽屉：详情/日志/环境 三 Tab（四种资源共用，滚动容器用 @material ScrollView+ListView）
    ├── drawer-detail-row.slint 详情/环境行（kv / 代码盒 / 挂载 chip）
    ├── drawer-log-row.slint   日志行 delegate（虚拟化 ListView 的一行 = 一条日志）
    ├── dialogs.slint          PullDialog + CleanupDialog 弹窗（继承 DialogShell）
    ├── update-dialog.slint    自动更新弹窗：版本对比 / 更新说明 / 进度 / 开关 / 安装与重启
    ├── settings-dialog.slint  设置弹窗：外观三选 + 自动更新三个开关（check/install/restart）
    ├── update-state.slint     自动更新 UI 状态 global（TopBar / UpdateDialog / Rust 共享）
    ├── ui-state.slint         显示态 global（TopBar / FootBar / MainArea / ListArea 与 Rust 共享）
    ├── menu-layers.slint      浮动菜单层：排序菜单 + 行「···」菜单（须在列表区之后渲染，否则被列表 clip 裁掉）
    └── components-*.slint     原子组件库按职能拆分：interact / nav / form / chip / align / state / empty / feedback / dialog
```

</details>

### 3.1 依赖方向（严格单向，无循环）

```text
main.rs
  ├── consumer.rs    → ui_state + ui_update + ui_rows + detail_fmt + render + ui_apply + slint_gen
  ├── bootstrap.rs   → docker::{fetch_stats, inspect_container, list_*, version, with_timeout}
  ├── ui_state.rs    → models.rs + docker::{inspect_container, list_*, with_timeout} + follow_runtime
  │                   └ ui_state/log_sync.rs（子模块：日志窗口裁剪 + 行模型同步账）
  ├── follow_runtime.rs → docker::{follow_logs, probe_container, retry_delay, LogError, LogStreamEnd, ContainerReach} + log_status + models::Msg
  ├── ui_update.rs   → ui_state + models::Msg + docker::compute_cpu_delta
  ├── ui_apply.rs    → slint_gen (MainWindow) + detail_fmt::DrawerData + render + ui_state::SharedState
  ├── callbacks.rs   → slint_gen + ui_state::SharedState + docker + config
  ├── callbacks_container.rs / callbacks_image.rs → 同 callbacks.rs
  ├── callbacks_update.rs → update（spawn_check / spawn_install）+ ui_apply（status_toast）
  ├── update/        → discover（三级回退 + `version.json.sig` 验签）+ manifest（清单纯逻辑 + `checksums.txt.sig`）+ signature（内置公钥清单）+ github（curl）+ version（纯逻辑）+ install（阻塞 IO）+ models::Msg + i18n
  └── docker/        → 纯 async 层，不反依赖任何上层模块

ui/*.slint           app.slint 是唯一组装入口，import 其余子文件；
                     子文件依赖是**无环 DAG**：theme.slint 是叶子（所有文件都 import 它），
                     composition 依赖只向下（list-area→ledger/image-list/network-list/volume-list/skel、
                     ledger→ledger-row/ledger-tree-row/ledger-band/ledger-cols、
                     image-list/network-list/volume-list→components-list/models/components、
                     drawer→drawer-*-row、dialogs→components-dialog、material-theme→theme、
                     main-area→list-area/sub-bar/batch-bar），
                     不存在互相 import / 循环
```

两条硬规则：

- **Slint 子文件之间不互相 import**（只 import `theme` / `models` / `components*` 这类叶子）；跨文件动作一律经 `AppActions` global 或 delegate 上的 callback 属性注入。
- **`docker/` 与 `update/` 是叶子方向**：只被上层调用，自己绝不 `use crate::ui_*`。

## 4. 文件规范

所有源码文件正文（去除头部 / 注释 / 空行）**≤ 300 行**（规则 R2）。

| 主题 | 拆分原则 | 权威源 |
|------|----------|--------|
| Slint | `app.slint` 只做组装层，子文件不互相 import | [arch/ui-interaction.md](arch/ui-interaction.md) §2 |
| Rust | consumer 独占 Receiver，分批 drain + 120ms 节流两阶段泵；所有 spawn 走 `state.rt`（锁内 clone handle → drop → spawn）；所有 Docker 结果走 `tx.send(Msg)`，不直接碰 Slint Model | [arch/threading.md](arch/threading.md) §4 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [文档风格与排版规范](style-guide.md) | [README 文档导航](../README.md#文档地图) | [线程模型](arch/threading.md) |
