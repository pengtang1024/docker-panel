# 更新日志

本文件记录 dvkit 的重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.0] - 2026-10-12

首个版本。

### 新增

- 容器管理：列表、搜索过滤、列排序、自定义显示列、多选批量启停重启删除、行内操作与右键菜单。
- 镜像管理：列表、拉取镜像（实时分层进度）、单条与批量删除。
- Compose 分组：按 compose 项目折叠分组，折叠态在会话内保持。
- 日志窗口：tail 200 行起步实时跟随，可暂停滚动与清空，上限 5000 行。
- 资源监控窗口：CPU / 内存指标卡与折线小图，每 2 秒采样，保留约 4 分钟历史。
- 简易清理：已停止容器、悬空镜像、未使用镜像可自由勾选，执行前警告、完成后汇报释放空间。
- 无感刷新：列表刷新保留上一次资源数据，CPU 超过 80% 自动标红。
- GitHub Dark 风格暗色主题，内置 Material Icons，启动时自动注入系统中文字体。
- 顶部 Docker 连接状态角标；破坏性操作全部二次确认。
- GitHub Actions：`CI`（fmt / clippy / build）与 `Release`（推送 `X.Y.Z` tag 自动构建 deb 与二进制并创建 Release）。

### 工程

- 架构：eframe UI 线程 + tokio 后台任务 + `std::mpsc` 消息泵，UI 全程无阻塞。
- 表格渲染期只收集 `TableOutcome`，渲染结束后统一应用，规避 egui 借用冲突。
- 资源采样并发上限 4，且上次未完成时不发起新轮询。
- 单文件不超过 300 行，按职责拆分为 `app/`、`docker/`、`utils/`、`widgets/` 四层模块。

[0.1.0]: https://github.com/pengtang1024/dvkit/releases/tag/0.1.0