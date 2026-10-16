# Docker Panel

极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、**纯软件渲染器**——零 GPU 依赖、零 Electron、零运行时。

[![Release](https://img.shields.io/github/v/release/pengtang1024/docker-panel?label=Release)](https://github.com/pengtang1024/docker-panel/releases/latest)

文档导航：[架构设计](docs/architecture.md) · [Git Flow 规范](docs/git-flow.md) · [AGENTS.md](AGENTS.md)（AI 协作入口）· [更新日志](CHANGELOG.md)

## 功能特性（v0.2.0 极简收敛版）

### 容器 Tab

- 容器列表：运行状态指示灯、短 ID、名称、镜像、状态文本
- 即时搜索过滤（名称 / ID / 镜像 / 状态）
- 行内操作按钮：启停（▶/⏹）、重启（⟳）、删除（🗑）
- 一键停止全部运行中容器
- 3 秒自动刷新 + 手动刷新按钮

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
3. 每个容器行右侧按钮：▶/⏹ 切换状态、⟳ 重启、🗑 删除。
4. 切到镜像 Tab 查看和管理镜像。
5. 工具栏：停止全部运行中容器 / 拉取镜像 / 清理未使用资源。
6. 底部状态栏显示最近操作结果。

## 架构概览

**三线程架构**：主线程（Slint 事件循环）+ consumer 线程（消息泵 + `invoke_from_event_loop`）+ tokio runtime（bollard async）。

```
主线程 ui.run()          consumer 线程              tokio runtime
┌────────────┐          ┌──────────────┐          ┌──────────────┐
│ Timer 3s   │ lock state│ recv() 阻塞   │          │ spawn 异步任务│
│ UI 回调    │─────────▶│ drain_messages│          │ bollard API   │
│            │          │ build rows    │          │ tx.send(Msg)  │
│            │          │ drop lock     │          └──────────────┘
│            │          │ invoke_from    │              ▲
│            │          │ _event_loop   │              │
│            │          └──────┬───────┘              │
│            │                 │ 更新 Slint Model ──────┘
└────────────┘
       Mutex<AppState>         mpsc::Receiver<Msg>（独占，不进 Mutex）
```

关键设计：`Receiver<Msg>` 由 consumer 独占所有权，**不**放进 Mutex。这样 `rx.recv()` 永远不阻塞主线程拿 state 锁。锁持有时间 = drain + build rows（微秒级），`invoke_from_event_loop` 必须在 drop lock 之后调用。

详见 [docs/architecture.md](docs/architecture.md)。

## 项目结构

```
docker-panel/
├── Cargo.toml              # 依赖与 build profile
├── Cargo.lock
├── build.rs                # slint_build::compile("ui/app.slint")
├── ui/                     # Slint UI 声明（5 文件按职责拆分）
│   ├── app.slint           # 主骨架：MainWindow + AppActions global，import 组装其余 4 个文件
│   ├── models.slint        # ContainerRow / ImageRow struct（Rust ↔ Slint 数据桥）
│   ├── delegates.slint     # 行 delegate（ListView 每行视图），callback 属性由 MainWindow 注入
│   ├── panes.slint         # TopBar / ToolBar / StatusBar 三个横向 strip 组件
│   └── dialogs.slint       # PullDialog + CleanupDialog 弹窗组件
├── AGENTS.md               # AI 协作入口（首次进入仓库请先读）
├── CHANGELOG.md            # 更新日志
├── docs/
│   ├── architecture.md     # 架构设计（线程模型 + Slint 交互图 + 文件规范）
│   └── git-flow.md         # Git Flow 分支与发版规范
├── packaging/              # deb 打包文件
│   ├── debian/control
│   ├── docker-panel.desktop
│   └── docker-panel.svg
└── src/
    ├── main.rs             # 桥接层（consumer 线程 + 定时刷新 + 回调绑定）
    ├── ui_state.rs         # AppState + 消息处理 + 格式化工具
    ├── models.rs           # 纯数据结构（容器、镜像、消息枚举）
    └── docker/
        ├── mod.rs          # connect() + pub use
        ├── containers.rs   # 容器 CRUD + 资源采样
        ├── images.rs       # 镜像列表 / 拉取 / 删除
        └── system.rs       # prune 清理 / follow_logs
```

## 主要依赖

| crate | 用途 |
|-------|------|
| slint 1.14 | 原生 GUI（`backend-winit` + `renderer-software`） |
| bollard 0.21 | Docker Engine API 客户端 |
| tokio | 异步运行时（多线程 + macros + time + sync） |
| futures | 流式处理 + `join!` |

## 许可证

MIT，见 [LICENSE](LICENSE)。
