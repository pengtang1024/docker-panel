# Docker Panel

极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、**纯软件渲染器**——零 GPU 依赖、零 Electron、零运行时。

[![Release](https://img.shields.io/github/v/release/pengtang1024/docker-panel?label=Release)](https://github.com/pengtang1024/docker-panel/releases/latest)

文档导航：[架构设计](docs/architecture.md) · [规则配置层](docs/agents-rules.md) · [Git Flow 规范](docs/git-flow.md) · [AGENTS.md](AGENTS.md)（AI 协作入口）· [更新日志](CHANGELOG.md)

## 功能特性

### 容器台账 + 三视图卡片网格

- 容器视图是**密集台账**：概览带（总数 / 运行中 / 已停止 / 需关注 + CPU 占用 TOP4）→ 表头 → 30px 虚拟化台账行
- 台账列：状态点、容器名、CPU（微条 + %）、内存（微条 + 容量）、磁盘、镜像、端口、Compose 项目、行内操作
- 列起点严格固定（表头与每一行逐列同 x 同宽），CPU / 内存微条按**当前视图内最大值**归一，同列一眼比高低
- 表头可点击排序（容器名 / CPU / 内存 / 磁盘），列宽随窗口响应式收缩（窄窗自动隐去磁盘 / 端口 / 项目列）
- 「需关注」= 死亡 / 暂停 / 重启中 / 正在移除 / 异常退出（退出码 ≠ 0），行首琥珀色状态轨标记
- 镜像 / 网络 / 卷仍是自适应列数卡片网格，筛选栏一键切换视图（Ctrl+1~4）
- 顶部筛选 chips（全部 / 运行中 / 已停止 / Compose 项目），网络 / 卷卡片支持搜索过滤（名称 / driver / 挂载点）

### 右侧抽屉（详情 / 日志 / 环境）

- 点击容器台账行 → inspect 详情（18 字段）+ 实时日志流（虚拟化列表，滚动窗口 2000 行）+ 环境变量 Tab
- 点击镜像卡片 → 镜像详情（RepoTag / 大小 / Entrypoint / Cmd / Env 等）

### 操作

- 台账行内按钮：启停（▶/⏹）、重启（⟳）、日志（📜）、更多（···）；「···」菜单同样可用
- 容器多选 + 批量操作浮动栏（启动 / 停止 / 删除）
- 顶栏：拉取镜像（实时进度弹窗）/ 清理未使用资源（已停止容器 / 悬空镜像 / 未使用镜像组合 prune）
- 操作完成立即刷新列表，不等 3s 定时周期

### 全局快捷键

- Ctrl+K 聚焦搜索、Ctrl+R 刷新、Ctrl+1~4 切换视图、Esc 关闭抽屉 / 弹窗

### 主题与本地化

- 浅色 / 深色 / 跟随系统 三态主题，22 个语义色令牌
- 内置 Material 3 组件库（`@material`，ScrollView / ListView）的色板由项目令牌映射而来，跟随应用主题而非系统主题
- 界面文案中英双语，跟随系统 locale 自动选择（zh* → zh_CN）

### 配置持久化

- 主题 / 视图 / 筛选 / 排序 / 搜索词 / 日志跟随 持久化到用户配置目录，启动自动恢复

### 连接状态

- 顶栏 Docker 连接角标：● 已连接 / ○ 未连接
- 底部栏 8 项资源概览 + 最近操作结果

## 环境要求

- Linux / macOS / Windows（主要在 Linux 上验证）
- 本机已安装并运行 Docker，默认通过 unix socket `/var/run/docker.sock` 连接；也支持 `DOCKER_HOST` 指定远程守护进程
- 构建需要 Rust stable（edition 2021）

## 安装与构建

### Debian / Ubuntu（deb 包）

从 [Releases](https://github.com/pengtang1024/docker-panel/releases) 下载 `docker-panel_<版本>_amd64.deb`：

```bash
sudo dpkg -i docker-panel_<版本>_amd64.deb
```

安装后在应用菜单中找到 Docker Panel；若点了没反应，多半是当前用户没有 Docker socket 权限：

```bash
sudo usermod -aG docker $USER
```

### 从源码构建

Linux 上 Slint software renderer 只依赖系统字体，不需要 libgl1：

```bash
# 调试运行
cargo run

# 发布构建（体积最小化 profile：opt-level=z + LTO + strip + panic=abort）
cargo build --release
./target/release/docker-panel
```

## 使用说明

1. 启动后默认显示容器台账视图，Docker 连接成功会自动加载。
2. Ctrl+K 聚焦搜索框，输入关键词即时过滤；Ctrl+R 手动刷新。
3. 容器台账行内按钮：▶/⏹ 切换状态、⟳ 重启、📜 查看日志、··· 更多操作。
4. 点击容器台账行 → 右侧抽屉显示详情 / 日志 / 环境；点击镜像卡片 → 镜像详情。
5. Ctrl+1~4 切换 容器 / 镜像 / 网络 / 卷 视图；Esc 关闭抽屉或弹窗。
6. 顶栏按钮：切换主题 / 刷新 / 清理 / 拉取镜像。
7. 底部栏显示 8 项资源概览与最近操作结果。

## 架构概览

**三线程架构**：主线程（Slint 事件循环）+ consumer 线程（消息泵 + `invoke_from_event_loop`）+ tokio runtime（bollard async）。定时刷新走 tokio interval（软件渲染器下 Slint Timer 不触发）。

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ UI 回调（搜索/排序/    │       │ recv_timeout(≤100ms) 循环 │       │ 3s 刷新 interval    │
│  操作/快捷键）──lock─▶│ 分批 drain（≤2048 条/批） │       │ 6s stats interval   │
│ 窗口关闭 → shutdown   │ 按 120ms 窗口合并 UI 更新  │       │ spawn 异步任务      │
│   flag                │ 只重建变化区域（日志/列表）│◀──────│ bollard API 调用    │
│                        │ drop(s) ──────────────────lock rel│ tx.send(Msg)        │
│                        │ invoke_from_event_loop     │       │ follow_logs 流      │
│                        │   render 缓存 + apply_drawer       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
   + shutdown: AtomicBool            （consumer 独占，不进 Mutex）
```

**关键设计**：
- `Receiver<Msg>` 由 consumer 独占所有权，**不**放进 Mutex。这样 `rx.recv_timeout()` 永远不阻塞主线程拿 state 锁。
- **两阶段铁律**：锁内只做分片 drain（单批 ≤2048 条）+ build → drop lock → 锁外 `invoke_from_event_loop` 更新 Slint Model。闭包里绝不碰 state。
- **UI 更新节流**：consumer 最多每 120ms 应用一次 UI；日志洪峰时只**追加**日志行模型，不重算容器/镜像/网络/卷/抽屉，避免无界 invoke 与重排把 UI 线程压死。
- **日志行模型只追加不重建**：行模型是日志窗口的镜像（滚动窗口 2000 行），能追加就追加（可见行不重排 → 不卡），窗口裁剪过前段才整体重建一次。
- **Docker 调用有超时 + 刷新去重**：单次调用 10s 超时；上一次 3s 刷新未完成时跳过下一轮，防止守护进程无响应时任务叠加耗尽 tokio 线程。
- **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。

详见 [docs/architecture.md](docs/architecture.md)。

## 项目结构

```
docker-panel/
├── Cargo.toml              # 依赖：slint 1.18.1 / bollard 0.21 / tokio / futures
├── Cargo.lock
├── build.rs                # slint_build::compile_with_config + with_bundled_translations("translations")
│
├── ui/                     # Slint UI 声明（31 文件按职责拆分，app.slint 唯一组装入口）
│   ├── app.slint           # MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 布局组装
│   ├── app-actions.slint   # AppActions global：UI → Rust 的唯一回调入口
│   ├── theme.slint         # 浅/深双主题 22 语义色令牌 + 字号/间距，theme_mode 三态
│   ├── material-theme.slint # M3 色板映射：Theme.* → MaterialScheme（@material 的 ScrollView/ListView 跟随应用主题）
│   ├── models.slint        # ContainerRow / ImageRow / NetworkRow / VolumeRow / DetailPair / LogRow / LedgerSummary struct
│   ├── top-bar.slint       # TopBar：logo + 连接状态 + 搜索框(Ctrl+K) + 主题/刷新/清理/拉取按钮
│   ├── sub-bar.slint       # SubBar：筛选 chips + 排序 + 4 视图 tabs
│   ├── cards.slint         # 资源列表区：容器视图走台账，镜像/网络/卷走卡片网格（含空状态与背景点击层）
│   ├── ledger.slint        # 容器台账区：列宽计算 + 表头 + @material ListView 虚拟化台账行
│   ├── ledger-row.slint    # 台账行 delegate（30px）：状态轨 / 勾选框 / 微条 / 行内操作
│   ├── ledger-band.slint   # 台账概览带：4 个计数 + CPU 占用 TOP4
│   ├── ledger-cols.slint   # 台账列宽契约 global（表头与行共用同一组列宽）
│   ├── card-image.slint    # 镜像卡片（含运行/删除操作）
│   ├── card-netvol.slint   # 网络/卷卡片（共用布局，内建标记 + 容器数）
│   ├── skel.slint          # 骨架屏占位
│   ├── foot-bar.slint      # 底部 8 项资源概览
│   ├── batch-bar.slint     # 批量操作浮动栏（选中容器才显示）
│   ├── drawer.slint        # 右侧抽屉：详情/日志/环境 三 Tab（四种资源共用）
│   ├── drawer-detail-row.slint # 详情/环境行（kind 区分 kv / 代码盒 / 挂载 chip）
│   ├── drawer-log-row.slint # 日志行 delegate（虚拟化 ListView 的一行 = 一条日志）
│   ├── dialogs.slint       # PullDialog + CleanupDialog 弹窗
│   ├── menu-layers.slint   # 浮动菜单层：排序菜单 + 卡片「···」菜单（须在卡片区之后渲染）
│   ├── components-*.slint  # 原子组件库按职能拆分：interact / nav / form / chip / align / state / empty / feedback / dialog
│
├── src/                    # Rust 层（所有 ≤ 300 行）
│   ├── main.rs             # 组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs        # slint::include_modules!() 集中入口
│   ├── i18n.rs             # locale 检测（zh*→zh_CN）+ Rust 侧 t()/trf()/trfp() + slint_language()
│   ├── config.rs           # AppConfig 持久化（directories + 手写 JSON 原子写）
│   ├── consumer.rs         # consumer 线程：分批 drain + 120ms UI 节流 + 变化区域重建 + 3s 刷新 interval
│   ├── bootstrap.rs        # 6s stats 采样 interval + 首屏四列表加载 + inspect 首容器
│   ├── models.rs           # Msg(+StatsBatch/LogBatch/Networks/Volumes) / ContainerInfo / ImageInfo / ContainerDetail 等
│   ├── ui_state.rs         # AppState + 四列表刷新（去重/超时）+ 日志流状态字段 + cleanup_counts + persist
│   ├── ui_state/log_sync.rs # 日志窗口裁剪 + UI 行模型同步账（LogSyncPlan：Append / Reset）
│   ├── follow_runtime.rs    # 日志流运行时：建流/退避重连/探针判定/保活降级/结局分类
│   ├── log_status.rs       # 日志流状态提示文案唯一定义处（3 条 i18n key）
│   ├── render.rs           # CachedModel 差量同步 + 日志行模型 Append/Reset 同步
│   ├── detail_fmt/         # 抽屉详情格式化：mod（公共 kv/head/format_bytes）+ container / image / netvol
│   ├── ui_rows/            # 行构建：mod（hit_any 搜索）+ containers（容器台账行）/ ledger（概览带）/ images / netvol（网络/卷）
│   ├── ui_update.rs        # DrainResult + 有上限 drain_messages + Msg → 变化区域聚合（日志洪峰防护）
│   ├── ui_apply.rs         # apply_drawer / refresh_lists_now / clear_log_panel / clear_log_status / status_toast
│   ├── batch.rs            # BatchState 多选状态
│   ├── callbacks.rs        # 回调绑定中枢 + restore_config
│   ├── callbacks_container.rs  # AppActions container_* 回调
│   ├── callbacks_drawer.rs     # 抽屉 Tab 切换（日志 Tab 触发 ensure_follow）+ 跟随/复制/清空
│   ├── callbacks_image.rs      # AppActions image_* 回调
│   ├── callbacks_netvol.rs     # 网络/卷 select / remove 回调
│   └── docker/
│       ├── mod.rs          # connect() + with_timeout（10s 调用上限）+ pub use 重导出
│       ├── containers.rs   # list / start / stop / restart / remove
│       ├── images.rs       # list / pull（含进度闭包）/ remove
│       ├── logs.rs         # follow_logs（Stream → 行批 tx.send，含空闲超时 + 复用定时器）
│       ├── log_types.rs    # 日志流结束原因 / 错误分类 / 退避常量（纯类型，单测不依赖 Docker）
│       ├── stats.rs        # fetch_stats + compute_cpu_delta（双采样 CPU%）
│       ├── netvol.rs       # list_networks / list_volumes
│       ├── image_details.rs # inspect_image → ImageDetail
│       ├── container_detail.rs # inspect_container + probe_container（存活性探针）
│       └── system.rs       # cleanup prune
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po   # gettext 翻译（编译期嵌入）
│
├── docs/
│   ├── architecture.md     # 架构总纲（技术栈 + 模块地图 + 文件规范 + 主题文档导航）
│   ├── arch/               # 架构主题文档：threading / ui-interaction / docker-bollard
│   ├── agents-rules.md     # 规则配置层（规则地图 + 红线约束 + 四阶段门禁 + 冲突记录）
│   └── git-flow.md         # Git Flow 分支与发版规范
├── packaging/
│   ├── debian/control
│   ├── docker-panel.desktop
│   └── docker-panel.svg
├── AGENTS.md               # AI 协作入口（首次进入仓库请先读）
├── CHANGELOG.md            # 更新日志
└── README.md
```

## 主要依赖（精确版本）

| crate | 精确版本 | Feature / 用途 |
|-------|----------|----------------|
| slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std` |
| slint-build | **1.18.1** | build-dependency，`compile_with_config` + bundled translations |
| bollard | **0.21** | Docker Engine API 纯 async 客户端 |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false） |
| futures | **0.3** | — |
| futures-util | **0.3** | 提供 `StreamExt`（`.next().await`） |
| serde_json | **1.x** | bollard 内部序列化 |
| anyhow | **1.x** | — |

> 完整技术栈精确版本和版本特有 API 限制见 `AGENTS.md`。

## 许可证

MIT，见 [LICENSE](LICENSE)。
