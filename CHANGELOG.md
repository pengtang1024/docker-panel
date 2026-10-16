# 更新日志

本文件记录 Docker Panel 的重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

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

[0.2.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.0
[0.1.1]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.1
[0.1.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.0