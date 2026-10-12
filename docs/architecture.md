# dvkit 架构设计

本文档描述 dvkit 的整体架构、线程模型、消息流与关键设计决策。

## 1. 分层

| 模块 | 职责 | 依赖方向 |
|------|------|----------|
| `main.rs` | 程序入口，创建 eframe 原生窗口（默认 1000x700，最小 820x520） | → app |
| `app.rs` | 应用主状态 `DvApp`、Tab 渲染、弹窗、定时刷新、消息轮询 | → docker / models / utils |
| `docker.rs` | bollard 封装，全部 Docker API 调用（async） | → models / utils |
| `models.rs` | 纯数据结构，无外部依赖 | — |
| `utils.rs` | 格式化工具、状态着色、CJK 字体加载 | → egui |

依赖是单向的：`main → app → docker → models`，`utils` 为底层工具，不反向依赖业务模块。

## 2. 线程模型与消息流

```
                 ┌─────────────────────────────────────────┐
                 │              UI 线程 (eframe)            │
                 │  pump_messages()  poll_logs()            │
                 │  maybe_refresh()  → 渲染 → 弹窗          │
                 └───────┬──────────────────────▲───────────┘
              spawn 任务 │                      │ std::mpsc::Sender<Msg>
                         ▼                      │
                 ┌─────────────────────────────────────────┐
                 │        tokio 多线程运行时 (DvApp.rt)      │
                 │  list_containers / list_images /         │
                 │  sample_container / pull_image /          │
                 │  start/stop/restart/remove / cleanup      │
                 │  follow_logs（长驻日志流，独立 channel）    │
                 └─────────────────────────────────────────┘
```

关键点：

- **UI 永不阻塞**：`docker.rs` 的所有函数都是 async，只在 tokio 后台任务里执行；结果打包为 `Msg` 枚举经 `std::mpsc` channel 回传。
- **消息轮询**：`pump_messages()` 每帧用 `try_recv` 非阻塞排空队列；配合 `ctx.request_repaint_after(250ms)` 保证后台消息最迟 250ms 内被处理。
- **日志流独立通道**：日志窗口有自己的 `Sender<LogEvent>`，关闭窗口时通过 `JoinHandle::abort()` 中断长驻任务；channel 断开时 `follow_logs` 自行退出。
- **拉取进度回调**：`pull_image(spec, on_progress)` 通过闭包把进度文本转成 `Msg::PullProgress`，最多保留 200 条（超出裁掉前 100 条）。

## 3. 刷新节奏

| 数据 | 周期 | 触发方式 |
|------|------|----------|
| 容器列表 | 2s | `maybe_refresh()`，仅在非 loading 时发起 |
| 镜像列表 | 15s（仅 Images Tab） | 同上 |
| 资源采样 | 2s | `stats_pending == 0` 才发起，天然限流 |
| 日志 | 实时 | follow 流，事件在 `poll_logs()` 中排空 |
| 手动刷新 | — | 工具条「刷新」按钮立即触发 |

采样并发上限 4：`futures::stream::iter(...).buffer_unordered(4)`。

## 4. UI 层设计决策

### 4.1 RowAction：规避 egui 借用冲突

表格渲染闭包里持有 `&self` 的不可变借用，不能同时改 `self`（如 `self.confirm = ...`）。因此行内按钮点击只往 `actions: Vec<RowAction>` 里 push，等表格渲染完后统一 match 执行。

### 4.2 列表刷新保留资源数据

`Msg::Containers(Ok(list))` 处理时，先按容器 ID 建立旧数据哈希表，把 `cpu_percent / mem_usage / ...` 等采样字段从旧记录拷贝到新记录，避免刷新瞬间指标闪归零。

### 4.3 弹窗状态机

| 字段 | 类型 | 用途 |
|------|------|------|
| `confirm` | `Option<Confirm>` | 删除容器/镜像的二次确认 |
| `error` | `Option<String>` | Docker API 错误；配合 `last_error` 去重，同一错误不重复弹 |
| `result` | `Option<String>` | 操作/清理结果信息 |
| `cleanup_open` | `bool` + 三个勾选 | 清理弹窗 |

弹窗统一在 `ui()` 末尾渲染，锚定屏幕中心。

### 4.4 资源监控小图

`draw_chart()` 是一个 110px 高的简易折线图：背景 + 3 条水平网格线 + 数据曲线 + 右上角当前值。CPU 曲线自适应最大值（历史峰值 ×1.2，下限 10%），内存曲线固定 0–100%（按容器内存上限百分比）。历史保留 120 个点 ≈ 4 分钟。

## 5. Docker 接入约定

- 连接统一走 `docker::connect()` → `Docker::connect_with_local_defaults()`：自动读 `DOCKER_HOST`，默认本机 unix socket。
- 每次 API 调用新建连接（bollard 连接轻量，且避免跨任务持有状态）。
- 错误统一转成中文 `String`（如 `"获取容器列表失败: {e}"`）上抛，由 UI 层弹窗展示。
- 删除容器使用 `force=true`，保证运行中容器也能删。
- 清理未使用镜像用 `prune_images` 的 `dangling=false` 过滤器，等价 `docker image prune -a`。

## 6. 资源指标计算

- **CPU%**：`(cpu_total - precpu_total) / (system - presystem) × online_cpus × 100`，与 `docker stats` 公式一致；`stream=false` 取单次采样。
- **内存**：`usage` 扣除缓存（cgroup v2 取 `inactive_file`，v1 取 `total_inactive_file`）。
- **磁盘 IO**：读取 `blkio_stats.io_service_bytes_recursive` 按 Read/Write 累加；cgroup v2 下无此明细，显示 0（与 docker stats 行为一致）。
- **网络 IO**：汇总所有网卡的 `rx_bytes / tx_bytes`。

## 7. 中文字体加载

egui 默认字体不含 CJK。启动时 `load_cjk_fonts()`：

1. 扫描 `/usr/share/fonts`、`/usr/local/share/fonts`、`~/.local/share/fonts`、`~/.fonts`（递归 .ttf/.otf/.ttc）。
2. 按关键词优先级挑选：思源黑体 → Noto Sans/Serif CJK → 文泉驿 → Droid Sans Fallback → Uming。
3. 追加到 `FontFamily::Proportional` 和 `Monospace` 列表**末尾**，不覆盖 egui 默认字体。
4. 找不到字体时向 stderr 打警告，中文可能显示为方块。
