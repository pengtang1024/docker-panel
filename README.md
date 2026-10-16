# Docker Panel

极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、**纯软件渲染器**——零 GPU 依赖、零 Electron、零运行时。

[![Release](https://img.shields.io/github/v/release/pengtang1024/docker-panel?label=Release)](https://github.com/pengtang1024/docker-panel/releases/latest)

文档导航：[架构设计](docs/architecture.md) · [Git Flow 规范](docs/git-flow.md) · [AGENTS.md](AGENTS.md)（AI 协作入口）· [更新日志](CHANGELOG.md)

## 功能特性（v0.2.0 极简收敛版）

### 容器 Tab

- 容器列表：运行状态指示灯、短 ID、名称、镜像、状态文本
- 即时搜索过滤（名称 / ID / 镜像 / 状态）
- 行内操作按钮：启停（▶/⏹）、重启（⟳）、删除（🗑）、查看日志（📜）
- 一键停止全部运行中容器
- 3 秒自动刷新 + 手动刷新按钮

### 详情面板（右侧 TabWidget 第一页）

- 点击容器行自动 inspect，展示 17 个字段：ID、名称、状态、镜像、命令、创建时间、运行时长、端口映射、挂载、环境变量、工作目录、重启次数、网络模式、资源限制、健康检查、标签、驱动
- 数据随容器切换实时更新

### 日志面板（右侧 TabWidget 第二页）

- 点行内 📜 按钮或直接触发 follow_logs，Docker API 实时流
- 环形缓冲 500 行

### 镜像 Tab

- 镜像列表：短 ID、名称、tag、关联容器数、大小
- 搜索过滤
- 一键删除（行内 🗑）

### Pull 弹窗

- 输入 `nginx:latest` 等镜像名
- Docker API 实时进度显示
- 完成自动关闭弹窗

### Cleanup 弹窗

- 三个复选框：已停止容器 / 悬空镜像 (dangling) / 所有未使用镜像 (危险)
- 执行 prune 清理，状态文本汇报结果

### 连接状态

- 顶栏 Docker 连接角标：● 已连接 / ○ 未连接
- 底部状态栏显示最近操作结果

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

1. 启动后默认显示容器列表，Docker 连接成功会自动加载。
2. 顶栏搜索框输入关键词即时过滤；回车或点刷新按钮手动刷新。
3. 每个容器行右侧按钮：▶/⏹ 切换状态、⟳ 重启、🗑 删除、📜 查看日志。
4. 选中容器后右侧 TabWidget 显示**详情面板**（inspect 结果）和**日志面板**（实时 follow）。
5. 切到镜像 Tab 查看和管理镜像。
6. 工具栏：停止全部运行中容器 / 拉取镜像 / 清理未使用资源。
7. 底部状态栏显示最近操作结果。

## 架构概览

**三线程架构**：主线程（Slint 事件循环）+ consumer 线程（消息泵 + `invoke_from_event_loop`）+ tokio runtime（bollard async）。

```
主线程 (ui.run() 阻塞)          consumer 线程                  tokio runtime (多线程)
┌──────────────────────┐       ┌────────────────────────┐       ┌───────────────────┐
│ Slint Timer 3s ──lock─▶│ recv() 阻塞（锁外）           │       │ spawn 异步任务      │
│ 手动刷新 ───────lock─▶│ drain_messages            │       │ bollard API 调用    │
│ UI 回调 ────────lock─▶│ build_container_rows      │◀──────│ follow_logs 流      │
│                        │ build_image_rows          │       │ inspect_container   │
│                        │ build_detail_data          │       │                     │
│                        │ build_log_lines            │       │                     │
│                        │ drop state ────────────────lock release
│                        │ invoke_from_event_loop     │       │                     │
│                        │   apply_lists / apply_detail       │                     │
│                        │   set_log_lines / set_...  │       │                     │
└──────────────────────┘       └────────────────────────┘       └───────────────────┘
     Mutex<AppState>                 mpsc::Receiver<Msg>          spawn(async { ... })
                                     （consumer 独占，不进 Mutex）
```

**关键设计**：
- `Receiver<Msg>` 由 consumer 独占所有权，**不**放进 Mutex。这样 `rx.recv()` 永远不阻塞主线程拿 state 锁。
- **pump_and_apply 两阶段铁律**：锁内只做 drain + build（微秒级）→ drop lock → 锁外 `invoke_from_event_loop` 更新 Slint Model。闭包里绝不碰 state。
- **所有 Docker 结果走 `tx.send(Msg)`**——不直接碰 Slint Model，统一由 consumer pump 更新。

详见 [docs/architecture.md](docs/architecture.md)。

## 项目结构

```
docker-panel/
├── Cargo.toml              # 依赖：slint 1.14 / bollard 0.21 / tokio / futures
├── Cargo.lock
├── build.rs                # slint_build::compile("ui/app.slint")
│
├── ui/                     # Slint UI 声明（8 文件按职责拆分，app.slint 唯一组装入口）
│   ├── app.slint           # 主骨架：MainWindow + AppActions global + 布局（FixedLayout 左右分栏）
│   ├── models.slint        # ContainerRow / ImageRow / LogLineRow struct（Rust ↔ Slint 数据桥）
│   ├── delegates.slint     # 行 delegate（ContainerRowDelegate 含日志按钮 callback）
│   ├── headers.slint       # ContainerHeader / ImageHeader（列宽严格对齐 delegates）
│   ├── panes.slint         # TopBar / ToolBar / StatusBar 三个横向 strip
│   ├── dialogs.slint       # PullDialog + CleanupDialog 弹窗组件
│   ├── detail-panel.slint  # DetailRow + DetailPanel（17 字段 in-out 属性，右侧 Tab 第一页）
│   └── log-panel.slint     # LogLineDelegate + LogPanel（ListView 滚动，右侧 Tab 第二页）
│
├── src/                    # Rust 层（所有 ≤ 300 行）
│   ├── main.rs             # 桥接中枢：consumer 线程 + 定时刷新 + pump_and_apply
│   ├── slint_gen.rs        # slint::include_modules!() 集中入口（仅 7 行）
│   ├── models.rs           # Msg / ContainerInfo / ImageInfo / ContainerDetail
│   ├── ui_state.rs         # AppState + drain_messages + handle_one + build_detail_data
│   ├── ui_update.rs        # derive_update(Msg → UiUpdate) + UiUpdate 结构体
│   ├── ui_apply.rs         # apply_detail / apply_lists — set_d_xxx 和 set_xxx 抽离
│   ├── callbacks.rs        # 顶栏按钮 + PullDialog + CleanupDialog 回调绑定
│   ├── callbacks_container.rs  # AppActions container_* 回调（toggle/restart/remove/logs）
│   ├── callbacks_image.rs      # AppActions image_remove 回调
│   └── docker/
│       ├── mod.rs          # connect() + pub use 重导出
│       ├── containers.rs   # list / start / stop / restart / remove
│       ├── images.rs       # list / pull（含进度闭包）/ remove
│       ├── logs.rs         # inspect_container + follow_logs（Stream → tx.send）
│       └── system.rs       # cleanup prune
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
| slint | **1.14** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-2` + `std` |
| slint-build | **1.14** | build-dependency，`slint_build::compile("ui/app.slint")` |
| bollard | **0.21** | Docker Engine API 纯 async 客户端 |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false） |
| futures | **0.3** | — |
| futures-util | **0.3** | 提供 `StreamExt`（`.next().await`） |
| serde_json | **1.x** | bollard 内部序列化 |
| anyhow | **1.x** | — |

> 完整技术栈精确版本和版本特有 API 限制见 `AGENTS.md`。

## 许可证

MIT，见 [LICENSE](LICENSE)。
