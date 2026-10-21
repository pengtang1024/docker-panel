# Docker 接入层：接入约定与 bollard 0.21 坑条

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是 Docker 接入约定与 bollard 坑条的**唯一权威源**（AGENTS.md 只留指针，不维护副本）。

## 1. Docker 接入约定

- 连接：`Docker::connect_with_local_defaults()`，自动读 `DOCKER_HOST`，默认 `/var/run/docker.sock`
- 错误统一转中文 `String` 上抛，UI 层展示
- 删除容器 `force=true`；删除镜像 `force=true`，被引用时 Docker 拒绝
- `inspect_container`：`docker::inspect_container(&id).await → ContainerDetail`
- `follow_logs`：`.follow(true).tail(tail_lines)` 返回 Stream，**整批** `tx.send(Msg::LogBatch(epoch, lines))`（满 1024 行或距首行 60ms 投一批；逐行 send 在洪峰下会形成惊群）；额外接 `watch::Receiver`，换容器时 drop sender 让旧流在 `select!` 中退出；返回 `LogStreamOutcome { end, lines }`（`LogStreamEnd`：Cancelled/Eof/Idle/Error(LogError) + 已推送行数），`LogError` 保留 Docker HTTP 状态码用于判定致命错误；超过 `LOG_IDLE_TIMEOUT`（180s）无新日志判定为 Idle（连接挂死）；重连策略由 `follow_runtime` 负责；任务结束发 `Msg::LogStreamEnded(epoch)`。默认 tail=200，Clear/续流用 0
- 可重试调用统一 `docker::with_timeout`（10s）：列表 / stats / inspect / 卷占用；`refresh_inflight` 防止 3s 刷新叠加
- **裸 HTTP 例外**：`bollard` 的低层入口（`process*` / `build_request`）全是 `pub(crate)`，没有公开的裸请求 API → 守护进程返回、但 bollard **未建模**的接口只能自己发一条 HTTP/1.1（`docker/raw_http.rs`：Unix socket + chunked 解码 + `spawn_blocking`）。现役用例：`/system/df?type=volume` 的 `Volumes` 键（卷占用）——bollard 0.21 的 `SystemDataUsageResponse` 只建模了废弃的 `VolumeUsage` 键，而守护进程（API ≥1.25）返回的是 `Volumes` 数组 → 走 bollard 的 `df()` 拿到的是**空表**
- 清理：`cleanup(stopped, dangling, unused_net, unused_vol)` 四个开关与清理弹窗的四个复选框一一对应，可任意组合：`stopped→prune_containers` / `dangling→prune_images(dangling=true)` / `unused_net→prune_networks` / `unused_vol→prune_volumes`（不加 `all` 过滤器，Docker 默认只 prune 匿名卷，与弹窗回填的「匿名卷」计数同口径）

## 2. bollard 0.21 易踩坑速查

| 项目 | 实际类型 | 备注 |
|------|----------|------|
| `LogOutput` | `LogOutput::StdOut { message: Bytes }` | struct 变体，**不是** tuple |
| `ContainerStateStatusEnum` | 没有 Default | 用 `.as_ref().map(\|e\| e.to_string()).unwrap_or_default()` |
| `NetworkSettings.ports` | `Option<HashMap<String, Option<Vec<PortBinding>>>>` | 双层 Option |
| `PortBinding.host_port` | `Option<String>` | 不是 u16 |
| `LogsOptionsBuilder.tail` | 收 `&str` | 不是 `String` |
| `StatsOptionsBuilder` 路径 | `bollard::query_parameters::StatsOptionsBuilder` | **不是** `bollard::stats::*` |
| `list_networks` | 不用泛型参数 | `Some(ListNetworksOptionsBuilder::default().build())` |
| `list_volumes` | 需要 type annotation | `list_volumes(None::<ListVolumesOptions>)` |
| `prune_volumes` | 参数是 `Option<impl Into<PruneVolumesOptions>>`（泛型） | `None` 无法推断类型；用 `PruneVolumesOptionsBuilder::new().build()` 得到具体值再传 `Some(options)`。**注意** builder 不实现 `Into`，不能直接传 builder 本身 |
| `Network` struct 字段 | 没有 `containers` 映射 | driver/scope/name/id 都是 `Option<String>` |
| `ContainerStatsResponse.networks` | `Option<HashMap<String, ContainerNetworkStats>>` | `rx_bytes/tx_bytes` 是 **u64**（不是 Option） |
| `docker.stats()` | 返回 Stream（不是 future） | `.next().await` 取第一帧 |
| `docker.df()` | `SystemDataUsageResponse` 只建模了废弃的 `VolumeUsage` 键 | 守护进程（API ≥1.25）返回的是 `Volumes` 数组 → 走 bollard 解析出来的卷占用**恒空**；改走 `docker::raw_http::get("/system/df?type=volume")` 自己读 `Volumes` |
| 依赖 | `futures-util = "0.3"` | 提供 `StreamExt` 调 `.next().await` / `join_all` |

> 升级 bollard 版本时重新核对本表；新增坑条只写在本文件（触发 agents-rules.md §6.1 文档同步）。
