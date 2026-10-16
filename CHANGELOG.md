# 更新日志

本文件记录 Docker Panel 的重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.2.1] - 2026-10-16

### 新增

- **Stats 资源监控**：容器列表新增 **CPU% / 内存使用 / 网络流量** 三列。双采样算法（`compute_cpu_delta`）用 tokio 6s interval 并发 join_all 拉所有 running 容器 stats，首次采样前 6s 显示 0%。
- **网络 Tab + 卷 Tab**：主界面 Tab 栏从 2 个扩展到 **4 个**（容器/镜像/网络/卷），主骨架抽离为 `ui/list-area.slint`（TabBar + ListArea）。
- **Compose 分组**：容器行按 `compose_project` 排序 + 分组标题行渲染（📁 Compose Project: xxx），ListView delegate 内 `is_group_start` 属性触发分组行。
- **日志流面板**：右侧第二 Tab（LogPanel）用 bollard `follow_logs` Stream → `tx.send(Msg::LogLine)`，AppState.logs 环形缓冲 cap=500。
- **Labels section**：详情面板最后一节，`ContainerDetail.labels` 格式化为多行 `key=value` 字符串显示。
- **操作完成立即刷新**：ActionDone / PullDone / CleanupDone 消息到达后 consumer 线程在锁内直接 `s.refresh()`，不等 3s Timer 周期，容器启停/拉镜像/清理后列表秒级刷新。
- **优雅退出**：consumer 线程 `recv_timeout(200ms)` + `AtomicBool shutdown`，`ui.run()` 返回后设 flag → join consumer → state drop → Runtime drop 自动 shutdown。窗口关闭不再依赖 OS 回收。
- **Slint MCP debug_info**：build.rs 加 `with_debug_info(true)`，`SLINT_EMIT_DEBUG_INFO=1` 环境变量支持 Slint inspector。

### 架构调整

- **Slint Timer → tokio interval**：软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），改为 `rt_h.spawn(async { tokio::time::interval(...); loop { tick().await; } })`。定时刷新 3s + Stats 6s 都走 tokio interval。
- **Stats 必须 spawn 在 runtime 里**：`state.rt.handle().clone()` 先从 Mutex 里 clone handle + tx + running ids，drop lock 后 spawn。Stats 写入 `state.stats_prev: HashMap<String, ContainerStatsSnapshot>` 双采样缓冲。
- **跨文件 global 不可见**：Slint 1.14 的 AppActions global 必须在 app.slint 里定义，其他 .slint 文件通过 component callback property 注入（`ListArea.container_click(id) => { AppActions.container_select(id); }`）。
- **drain_messages 统一处理 Stats**：每条 Msg::Stats 先写回 containers（累积更新），再一次性 match 更新列表/Detail/Logs——去掉之前的 `handle_one` 10 参数函数。

### 工程重构

- **app.slint 354 → 129 行**：TabBar + ListArea 抽离到 `ui/list-area.slint`（137 行），DetailPanel + LogPanel + 自绘 Tab 抽离到 `ui/detail-log-panel.slint`（89 行）。
- **ui_state.rs 314 → 253 行**：去掉参数爆炸的 `handle_one` 函数。
- **新增 7 个文件**：
  - `src/docker/stats.rs`（88 行）：fetch_stats + compute_cpu_delta
  - `src/docker/netvol.rs`（53 行）：list_networks + list_volumes（bollard 0.21 API 坑条）
  - `src/ui_rows.rs`（132 行）：4 种 Slint 行构建 + Compose 分组排序
  - `ui/list-area.slint`：4 Tab + 4 个条件渲染列表区（callback 注入）
  - `ui/network-views.slint`：NetworkRowDelegate + VolumeRowDelegate
  - `ui/detail-log-panel.slint`：右侧详情/日志面板（条件渲染 + 自绘 Tab 头）
  - `ui/actions.slint`：AppActions global 定义（独立文件供跨文件可见性测试备用）

### bollard 0.21 坑条修复

- `StatsOptionsBuilder` 路径：`bollard::query_parameters::StatsOptionsBuilder`（不是 `bollard::stats::*`）
- `list_networks`：不用 `<String>` 泛型，用 `Some(ListNetworksOptionsBuilder::default().build())`
- `list_volumes`：`list_volumes(None::<ListVolumesOptions>)` 需要 type annotation
- `ContainerStatsResponse.networks`：`Option<HashMap<String, ContainerNetworkStats>>`，`rx_bytes/tx_bytes` 是 **u64**（不是 `Option<u64>`）
- `docker.stats()` 返回 Stream：`.next().await` 取第一帧

## [0.2.0] - 2026-10-16

### 完全重写

- **UI 框架**：从 egui/eframe → **Slint 1.14** 原生 GUI。
- **渲染器**：wgpu/Vulkan → **纯软件渲染器**（`renderer-software` + `systemfonts`），零 GPU 依赖。
- **二进制体积**：40M+ → **12M**（`opt-level="z"` + LTO + strip + `panic="abort"`）。
- **资源占用**：idle RSS **33MB**，idle CPU **≈ 0%**（事件驱动，consumer 线程 `rx.recv()` 阻塞，替代旧 200ms 轮询）。

### 功能收敛（极简 Docker Desktop Base）

保留核心容器/镜像管理，移除 v0.1.x 的 Compose 分组、日志窗口、资源监控折线图、行内排序、多选批量、右键菜单等非"桌面管理基础版"功能。

### 新增

- **Pull 弹窗**：输入镜像名 → Docker API 实时进度 → 完成自动关闭。
- **Cleanup 弹窗**：3 复选框（stopped/dangling/unused）→ 执行 prune → 结果汇报。
- **Docker 连接状态角标**。
- **Slint MCP server**（`--features slint/mcp` + `SLINT_MCP_PORT=8080`）：直接从 Slint software renderer 拿 framebuffer 做截图，绕过 Wayland/GNOME 安全链。

### 工程

- 架构：**主线程** `ui.run()` + `slint::Timer::Repeated(3s)` → 定时触发 refresh；**consumer 线程** `rx.recv()` 阻塞 → 收到消息 → `invoke_from_event_loop` 用 `Weak<MainWindow>` 升级后更新 Slint Model；**tokio multi_thread** 执行 bollard API。
- **模块**：`src/main.rs`（入口）+ `src/callbacks.rs`（UI 回调绑定）+ `src/ui_state.rs`（AppState + drain_messages）+ `src/models.rs` + `src/slint_gen.rs`（include_modules! 集中入口）+ `src/docker/*`（bollard 封装）+ `ui/*.slint`（Slint 标记分 **5 文件**：app 组装层 + models 数据桥 + delegates 行视图 + panes 横向 strip + dialogs 弹窗）+ `build.rs`。
- **文件 ≤ 300 行**：所有 `.rs` 和 `.slint` 文件正文均 ≤ 300 行。原 app.slint 387 行 → 先拆 3 文件（app/delegates/dialogs）→ 再拆 5 文件（加 models/panes），最终 app.slint 只剩 ~148 行骨架（纯 import 组装 + AppActions global + MainWindow 壳）；Slint 文件全部放 `ui/` 目录；delegates.slint 通过 callback 属性注入代替 AppActions global 直依赖，避免跨文件 global 可见性 + Rust glob import 问题。
- **Packaging**：`Depends` 移除 `libgl1`（软件渲染不需要）。

### 移除

- vendor/egui-0.36.2 目录（整个 egui crate 及 eframe、wgpu 传递依赖）。
- src/app/、src/widgets/、src/utils/ 三个旧模块。
- assets/MaterialIcons-Regular.ttf（Slint software-renderer-systemfonts 用系统字体）。
- 200ms 后台轮询循环。

[0.2.1]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.1
[0.2.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.0
[0.1.1]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.1
[0.1.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.0