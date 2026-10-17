# Docker Panel

极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、**纯软件渲染器**——零 GPU 依赖、零 Electron、零运行时。

[![Release](https://img.shields.io/github/v/release/pengtang1024/docker-panel?label=Release)](https://github.com/pengtang1024/docker-panel/releases/latest)

文档导航：[架构设计](docs/architecture.md) · [Git Flow 规范](docs/git-flow.md) · [AGENTS.md](AGENTS.md)（AI 协作入口）· [更新日志](CHANGELOG.md)

## 功能特性

### 四视图卡片网格

- 容器 / 镜像 / 网络 / 卷四种视图卡片式布局，筛选栏一键切换（Ctrl+1~4）
- 容器卡片：状态指示灯、短 ID、名称、镜像、CPU% / 内存 / 网络流量实时指标（6s 采样）、Compose 项目彩色标签
- Compose 项目分组排序，顶部筛选 chips（全部 / 运行中 / 已停止）
- 网络 / 卷卡片支持搜索过滤（名称 / driver / 挂载点）

### 右侧抽屉（详情 / 日志 / 环境）

- 点击容器卡片 → inspect 详情（18 字段）+ 实时日志流（环形缓冲 500 行）+ 环境变量 Tab
- 点击镜像卡片 → 镜像详情（RepoTag / 大小 / Entrypoint / Cmd / Env 等）

### 操作

- 卡片行内按钮：启停（▶/⏹）、重启（⟳）、删除（🗑）、日志（📜）；右键上下文菜单同样可用
- 容器多选 + 批量操作浮动栏（启动 / 停止 / 删除）
- 顶栏：拉取镜像（实时进度弹窗）/ 清理未使用资源（已停止容器 / 悬空镜像 / 未使用镜像组合 prune）
- 操作完成立即刷新列表，不等 3s 定时周期

### 全局快捷键

- Ctrl+K 聚焦搜索、Ctrl+R 刷新、Ctrl+1~4 切换视图、Esc 关闭抽屉 / 弹窗

### 主题与本地化

- 浅色 / 深色 / 跟随系统 三态主题，22 个语义色令牌
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

1. 启动后默认显示容器卡片视图，Docker 连接成功会自动加载。
2. Ctrl+K 聚焦搜索框，输入关键词即时过滤；Ctrl+R 手动刷新。
3. 容器卡片按钮：▶/⏹ 切换状态、⟳ 重启、🗑 删除、📜 查看日志；右键菜单同样可用。
4. 点击容器卡片 → 右侧抽屉显示详情 / 日志 / 环境；点击镜像卡片 → 镜像详情。
5. Ctrl+1~4 切换 容器 / 镜像 / 网络 / 卷 视图；Esc 关闭抽屉或弹窗。
6. 顶栏按钮：切换主题 / 刷新 / 清理 / 拉取镜像。
7. 底部栏显示 8 项资源概览与最近操作结果。

## 架构概览

**三线程架构**：主线程（Slint 事件循环）+ consumer 线程（消息泵 + `invoke_from_event_loop`）+ tokio runtime（bollard async）。定时刷新走 tokio interval（软件渲染器下 Slint Timer 不触发）。

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ UI 回调（搜索/排序/    │       │ recv_timeout(200ms) 循环 │       │ 3s 刷新 interval    │
│  操作/快捷键）──lock─▶│ drain_messages            │       │ 6s stats interval   │
│ 窗口关闭 → shutdown   │ build_*_rows ×4           │       │ spawn 异步任务      │
│   flag                │ detail_fmt::build_detail  │◀──────│ bollard API 调用    │
│                        │ drop(s) ──────────────────lock rel│ tx.send(Msg)        │
│                        │ invoke_from_event_loop     │       │ follow_logs 流      │
│                        │   apply_lists / apply_detail       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
   + shutdown: AtomicBool            （consumer 独占，不进 Mutex）
```

**关键设计**：
- `Receiver<Msg>` 由 consumer 独占所有权，**不**放进 Mutex。这样 `rx.recv_timeout()` 永远不阻塞主线程拿 state 锁。
- **pump_and_apply 两阶段铁律**：锁内只做 drain + build（微秒级）→ drop lock → 锁外 `invoke_from_event_loop` 更新 Slint Model。闭包里绝不碰 state。
- **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。

详见 [docs/architecture.md](docs/architecture.md)。

## 项目结构

```
docker-panel/
├── Cargo.toml              # 依赖：slint 1.18.1 / bollard 0.21 / tokio / futures
├── Cargo.lock
├── build.rs                # slint_build::compile_with_config + with_bundled_translations("translations")
│
├── ui/                     # Slint UI 声明（16 文件按职责拆分，app.slint 唯一组装入口）
│   ├── app.slint           # MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 布局组装
│   ├── theme.slint         # 浅/深双主题 22 语义色令牌 + 字号/间距，theme_mode 三态
│   ├── models.slint        # ContainerRow / ImageRow / LogLineRow / NetworkRow / VolumeRow struct
│   ├── top-bar.slint       # TopBar：logo + 连接状态 + 搜索框(Ctrl+K) + 主题/刷新/清理/拉取按钮
│   ├── sub-bar.slint       # SubBar：筛选 chips + 排序 + 4 视图 tabs
│   ├── cards.slint         # 容器/镜像/网络/卷四种资源卡片网格
│   ├── foot-bar.slint      # 底部 8 项资源概览
│   ├── batch-bar.slint     # 批量操作浮动栏（选中容器才显示）
│   ├── drawer.slint        # 右侧抽屉：详情/日志/环境 三 Tab
│   ├── detail-panel.slint  # DetailRow + 详情 kv section（drawer 复用）
│   ├── dialogs.slint       # PullDialog + CleanupDialog 弹窗
│   ├── delegates.slint     # 容器/镜像表格行（RowGroup/RowContainer/RowImage）
│   ├── network-views.slint # 网络/卷表格行（RowNetwork/RowVolume）
│   ├── headers.slint       # 各视图表头（列宽严格对齐 delegates）
│   └── components.slint    # IconBtn / Dot / CheckCell 等原子组件
│
├── src/                    # Rust 层（所有 ≤ 300 行）
│   ├── main.rs             # 组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs        # slint::include_modules!() 集中入口
│   ├── i18n.rs             # locale 检测（zh*→zh_CN）+ Rust 侧 t() + slint_language()
│   ├── config.rs           # AppConfig 持久化（directories + 手写 JSON 原子写）
│   ├── consumer.rs         # consumer 线程：recv_timeout Msg 泵 pump_and_apply + 3s 刷新 interval
│   ├── bootstrap.rs        # 6s stats 采样 interval + 首屏四列表加载 + inspect 首容器
│   ├── models.rs           # Msg(+Stats/Networks/Volumes) / ContainerInfo / ImageInfo / ContainerDetail 等
│   ├── ui_state.rs         # AppState + drain_messages + persist
│   ├── detail_fmt.rs       # 详情/镜像详情 → SharedString 元组格式化（format_bytes）
│   ├── ui_rows.rs          # 4 种行构建 + Compose 分组排序 + 网络/卷搜索过滤
│   ├── ui_update.rs        # derive_update(Msg → UiUpdate)
│   ├── ui_apply.rs         # apply_detail / apply_lists / refresh_lists_now
│   ├── batch.rs            # BatchState 多选状态
│   ├── callbacks.rs        # 回调绑定中枢 + restore_config
│   ├── callbacks_container.rs  # AppActions container_* 回调
│   ├── callbacks_image.rs      # AppActions image_remove 回调
│   └── docker/
│       ├── mod.rs          # connect() + pub use 重导出
│       ├── containers.rs   # list / start / stop / restart / remove
│       ├── images.rs       # list / pull（含进度闭包）/ remove
│       ├── logs.rs         # inspect_container / follow_logs（Stream → tx.send）
│       ├── stats.rs        # fetch_stats + compute_cpu_delta（双采样 CPU%）
│       ├── netvol.rs       # list_networks / list_volumes
│       ├── image_details.rs # inspect_image → ImageDetail
│       └── system.rs       # cleanup prune
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po   # gettext 翻译（编译期嵌入）
│
├── docs/
│   ├── architecture.md     # 架构设计（线程模型 + Slint 交互图 + 文件规范 + bollard 坑条）
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
