# dvkit

极简 Docker 桌面管理工具，基于 Rust + egui 原生 GUI。单二进制、无 Electron、无运行时依赖，启动即用。

## 功能特性

### 容器管理（Containers Tab）
- 容器列表：短 ID、名称、镜像、状态、运行时长、端口映射
- 实时资源列：CPU%、内存占用（含百分比）、磁盘读写、网络 IO
- 筛选：仅正在运行 / 全部容器；手动刷新；自动刷新（每 2 秒）
- 行内操作：启动、停止、重启、删除（删除前二次确认，运行中容器强制删除）
- 日志窗口：实时跟随日志流，可暂停滚动、清空，最多保留 5000 行
- 资源监控窗口：CPU / 内存折线小图，每 2 秒采样，保留约 4 分钟历史

### 镜像管理（Images Tab）
- 镜像列表：名称、Tag、大小、创建时间、关联容器数；悬空镜像黄色高亮
- 拉取镜像：输入 `nginx:latest` 等规格，实时显示拉取进度
- 删除镜像：二次确认；被容器引用的镜像由 Docker 拒绝删除
- 自动刷新（15 秒）

### 简易清理
- 可勾选组合：已停止的容器 / 悬空镜像（dangling）/ 未使用镜像
- 执行前列出不可恢复警告，完成后汇报删除数量与释放空间

## 环境要求

- Linux / macOS / Windows
- 本机已安装并运行 Docker（默认通过 unix socket `/var/run/docker.sock` 连接，也可设置 `DOCKER_HOST`）
- 构建需要 Rust stable（edition 2021，已在 rustc 1.99 验证）

## 构建与运行

```bash
# 调试运行
cargo run

# 发布构建
cargo build --release
./target/release/dvkit
```

> 若系统缺少中文字体，界面中文会显示为方块。程序启动时会自动扫描
> `/usr/share/fonts` 等目录，优先加载思源黑体 / Noto CJK / 文泉驿。

## 使用说明

1. 启动后默认进入 **Containers** 页，只显示正在运行的容器；点「全部容器」可看历史容器。
2. 行内按钮直接操作，破坏性操作（删除容器 / 镜像、清理）都会弹二次确认。
3. 点「日志」打开实时日志窗口，点「监控」打开该容器的 CPU / 内存曲线窗口。
4. **Images** 页顶部输入镜像规格回车即可拉取，进度显示在输入框下方。
5. 「清理...」打开清理弹窗，勾选项目后点「确认执行」。
6. 底部状态栏显示最近一次操作结果；Docker 调用失败会弹错误窗（同一错误不重复弹）。

## 架构概览

```
UI 线程 (egui/eframe)                    tokio 后台运行时
┌──────────────────────┐    mpsc channel  ┌──────────────────────┐
│ app.rs               │ ──── spawn ────▶ │ docker.rs (bollard)  │
│  每帧 pump_messages  │ ◀── Msg 回传 ─── │  list/sample/pull/…  │
│  定时 maybe_refresh  │                  │  全部 async 不阻塞UI │
└──────────────────────┘                  └──────────────────────┘
        models.rs（数据结构） / utils.rs（格式化、中文字体）
```

- 所有 Docker 调用都在 tokio 后台任务执行，结果经 `std::mpsc` channel 回传，UI 永不阻塞。
- 行内按钮先收集为 `RowAction`，表格渲染结束后统一执行，规避 egui 借用冲突。
- 资源采样并发上限 4（`buffer_unordered(4)`），且上一次采样未完成不会发起新的轮询。
- 列表刷新时保留上一次的资源数据，避免刷新瞬间指标归零。

详见 [docs/architecture.md](docs/architecture.md)。

## 项目结构

```
dvkit/
├── Cargo.toml          # 依赖与构建配置
├── src/
│   ├── main.rs         # 入口：创建 eframe 窗口
│   ├── app.rs          # GUI 主状态、Tab 渲染、各类弹窗
│   ├── docker.rs       # bollard 封装：全部 Docker API 调用
│   ├── models.rs       # 数据结构：ContainerInfo / ImageInfo / Msg 等
│   └── utils.rs        # 字节/时间格式化、状态着色、CJK 字体加载
└── docs/               # 项目文档
```

## 主要依赖

| crate | 用途 |
|-------|------|
| eframe / egui / egui_extras | 即时模式 GUI、表格 |
| bollard | Docker Engine API 客户端 |
| tokio | 多线程异步运行时 |
| futures | 流式处理（日志流、拉取进度、并发采样） |
| anyhow / serde_json | 错误与 JSON 处理 |

## 许可证

MIT，见 [LICENSE](LICENSE)。
