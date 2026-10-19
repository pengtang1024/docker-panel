# Docker 接入层：接入约定与 bollard 0.21 坑条

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是 Docker 接入约定与 bollard 坑条的**唯一权威源**（AGENTS.md 只留指针，不维护副本）。

## 1. Docker 接入约定

- 连接：`Docker::connect_with_local_defaults()`，自动读 `DOCKER_HOST`，默认 `/var/run/docker.sock`
- 错误统一转中文 `String` 上抛，UI 层展示
- 删除容器 `force=true`；删除镜像 `force=true`，被引用时 Docker 拒绝
- `inspect_container`：`docker::inspect_container(&id).await → ContainerDetail`
- `follow_logs`：`.follow(true).tail(tail_lines)` 返回 Stream，逐行 `tx.send(Msg::LogLine(epoch, line))`；额外接 `watch::Receiver`，换容器时 drop sender 让旧流在 `select!` 中退出；返回 `LogStreamOutcome { end, lines }`（`LogStreamEnd`：Cancelled/Eof/Idle/Error(LogError) + 已推送行数），`LogError` 保留 Docker HTTP 状态码用于判定致命错误；超过 `LOG_IDLE_TIMEOUT`（180s）无新日志判定为 Idle（连接挂死）；重连策略由 `follow_runtime` 负责；任务结束发 `Msg::LogStreamEnded(epoch)`。默认 tail=200，Clear/续流用 0
- 可重试调用统一 `docker::with_timeout`（10s）：列表 / stats / inspect / 卷占用；`refresh_inflight` 防止 3s 刷新叠加
- 清理：`stopped→prune_containers` / `dangling→prune_images(dangling=true)` / `unused→prune_images(-a)`，可任意组合

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
| `Network` struct 字段 | 没有 `containers` 映射 | driver/scope/name/id 都是 `Option<String>` |
| `ContainerStatsResponse.networks` | `Option<HashMap<String, ContainerNetworkStats>>` | `rx_bytes/tx_bytes` 是 **u64**（不是 Option） |
| `docker.stats()` | 返回 Stream（不是 future） | `.next().await` 取第一帧 |
| 依赖 | `futures-util = "0.3"` | 提供 `StreamExt` 调 `.next().await` / `join_all` |

> 升级 bollard 版本时重新核对本表；新增坑条只写在本文件（触发 agents-rules.md §6.1 文档同步）。
