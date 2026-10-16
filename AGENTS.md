# AGENTS.md — dvkit 开发协作入口

> 给 AI 编码助手看的项目速查手册。首次进入仓库请先读此文件。

## 项目是什么

**dvkit**（原 docker-panel）是一款极简 Docker 桌面管理工具。Rust 单二进制、Slint 原生 GUI、纯软件渲染器——零 GPU 依赖、零 Electron、零运行时。当前版本 v0.2.0。

## 技术栈

| 组件 | 选型 |
|------|------|
| 语言 | Rust stable（edition 2021） |
| GUI 框架 | Slint 1.14（`backend-winit` + `renderer-software` + `software-renderer-systemfonts`） |
| Docker API | bollard 0.21（纯 async） |
| 异步运行时 | tokio（多线程 + futures） |
| 渲染 | 纯软件渲染器，无 Vulkan/GL/WGPU 依赖 |
| 构建优化 | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip |

## 模块地图

```
dvkit/
├── Cargo.toml              # 依赖 & release profile
├── Cargo.lock
├── build.rs                # slint_build::compile("app.slint")
├── app.slint               # Slint UI 声明（主窗口 + 行 delegate + 弹窗）
├── src/
│   ├── main.rs             # 桥接层：consumer 线程 + 定时刷新 + 回调绑定（~440 行，下一步应拆分）
│   ├── ui_state.rs         # AppState + SharedState + drain_messages + derive_update + format_bytes
│   ├── models.rs           # ContainerInfo / ImageInfo / Msg / ResourceSample / ChartPoint / LogEvent
│   └── docker/
│       ├── mod.rs          # connect() + pub use 重导出
│       ├── containers.rs   # list_containers / start / stop / restart / remove / sample_container
│       ├── images.rs       # list_images / pull_image（含进度回调）/ remove_image
│       └── system.rs       # cleanup / follow_logs
├── docs/
│   ├── architecture.md     # 架构设计（线程模型、消息流、模块职责）
│   └── git-flow.md         # Git Flow 分支与发版规范
├── packaging/
│   ├── debian/control      # deb 包元信息（Depends 移除 libgl1，软件渲染不需要）
│   ├── docker-panel.desktop
│   └── docker-panel.svg
└── CHANGELOG.md
```

## 线程模型（三线程架构）

```
主线程 (ui.run() 阻塞)          consumer 线程               tokio runtime
┌───────────────────┐       ┌────────────────────┐       ┌──────────────────┐
│ Slint Timer 3s ── ┼──lock─▶│ recv() 阻塞（锁外） ┼──────▶│ spawn 异步任务     │
│ UI 回调 ──────────┼──lock─▶│ drain_messages()   │       │ bollard API 调用   │
│                   │       │ build rows          │       │ tx.send(Msg)       │
│                   │       │ drop lock ──────────┼─invoke─▶│                    │
│                   │       │   _from_event_loop  │       │                    │
└───────────────────┘       └────────────────────┘       └──────────────────┘
     Mutex<AppState>               mpsc::Receiver<Msg>（独占，不进 Mutex）
```

**关键设计**：`mpsc::Receiver<Msg>` 不在 Mutex 内部，由 consumer 线程独占。这样 `rx.recv()` 永远不会阻塞主线程拿 state 锁，彻底避免死锁。

锁持有时间 = `drain_messages` + `build rows`，微秒级；`invoke_from_event_loop` 必须在 drop lock 之后调用。

## 消息类型（`models::Msg`）

| 变体 | 来源 | UI 行为 |
|------|------|---------|
| `Containers(Result<Vec>)` | 定时刷新 / 手动刷新 | 替换容器列表 + 剪失效选择 |
| `Images(Result<Vec>)` | 定时刷新 / 手动刷新 | 替换镜像列表 + 剪失效选择 |
| `PullProgress(String)` | pull_image 进度回调 | 状态文本更新 |
| `PullDone(Result)` | pull_image 结束 | 关闭弹窗 + 刷新镜像列表 |
| `ActionDone(desc, Result)` | 启停重启删除等操作 | 状态文本 + 刷新容器/镜像 |
| `CleanupDone(Result)` | cleanup 完成 | 状态文本 + 刷新两个列表 |
| `Sample(id, Result)` | 暂未使用（v0.2 移除了资源监控） | — |

## 代码规范

1. **单文件 ≤ 300 行**——超过必须按职责拆分。`main.rs` 当前 ~440 行，下一步应把回调绑定移到 `callbacks.rs`。
2. **Mutex 锁持有最小化**——任何阻塞操作、跨线程调用、`invoke_from_event_loop` 都必须在 drop lock 之后。
3. **Receiver 不进 Mutex**——`mpsc::Receiver` 由 consumer 线程独占所有权，绝不能放回 `AppState`。
4. **命名**：Rust 常规 snake_case / PascalCase；Slint 中全局函数用 `snake_case`，结构体用 `PascalCase`。
5. **错误处理**：Docker API 错误统一转为 `Result<T, String>` 上抛，UI 层展示中文文本。

## 构建命令

```bash
# 开发调试
cargo run

# 发布构建（体积最小化 profile）
cargo build --release
./target/release/dvkit

# 代码检查
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
```

## 发版流程

严格走 Git Flow，详见 [docs/git-flow.md](docs/git-flow.md)。三个位置必须同步版本号：

1. `Cargo.toml` 的 `version`
2. `packaging/debian/control` 的 `Version`
3. `CHANGELOG.md` 新增版本段落

## 当前已知限制（v0.2.0）

- 无日志窗口（v0.1 有，v0.2 为收敛功能移除）
- 无资源监控折线图（同上）
- 无 Compose 分组
- 无行内排序、多选批量、右键菜单
- 搜索是即时本地过滤（好），但只对名称/ID/镜像/状态做粗粒度匹配
- 消费者线程 `spawn loop` 无优雅退出通道（channel 关闭才 break，窗口关闭时依赖 OS 回收）
- `main.rs` 超 300 行，待拆分
- **README / architecture.md 仍描述 v0.1 egui 版本，必须重写**

## 文档导航

- [架构设计](docs/architecture.md)
- [Git Flow 规范](docs/git-flow.md)
- [更新日志](CHANGELOG.md)
- [AGENTS.md](AGENTS.md)（本文件）
