# continuous-optimization-2 第 3 轮审查报告

审查范围：`git diff baseline-continuous-optimization-2..1ce68d5 -- . ':(exclude).opencode'`
审查 HEAD：1ce68d5（第 2 轮全部修复已入基线）
reviewer：独立 general 子代理（只读）
本轮修复落地：见文末「修复对照」

## 结论

**需修改**（无 🔴 逻辑缺陷、无 R2 红线违规；上轮 #9~#18 全部闭环，但新增 4 项 🟡 与 7 项 🟢）

上轮核对：#9 已解决（提示时机改为「重连后收到首行日志」，并由 `notified_recovered` 回传）；#10 已解决（`follow_is_alive` 现场计算 + `Msg::LogStreamEnded` 复位）；#11 已解决（`LOG_IDLE_TIMEOUT` 主动断流）；#12 已解决（单测改纯函数断言）；#13~#18 已解决（R8 补齐、状态栏独立通道、epoch 过滤、结构化状态码、冗余参数清理）。

## 新问题

### #19 🟡 `src/docker/logs.rs` 正文 309 行，超 R2（300 行）红线
单文件承担「流读取 + 结束原因 + 错误分类 + 退避常量」四件事。→ 拆出 `src/docker/log_types.rs`（纯类型/纯函数）。

### #20 🟡 Idle（空闲超时）与真实错误共用失败预算，且预算耗尽后静默放弃
挂死连接被当成「连续失败」扣预算；8 次 ≈55s 后直接放弃跟随，用户看到日志停在半截却没有任何提示。→ `FollowAction` 拆 `End` / `Recover{reason}` / `KeepAlive{reason}`：Idle 走 `KeepAlive`（静默重连、**不计失败预算、永不放弃**）；预算耗尽后转 30s 低频保活 `KEEPALIVE_INTERVAL`。

### #21 🟡 首次致命错误零 UI 提示
容器被删除时旧实现直接静默退出，用户看到的仍是「正常」的日志面板。→ `End` 分支除 `Cancelled` 外一律发「日志流已停止」。

### #22 🟡 「已恢复连接」只依赖首行日志 + `notified` 单向锁存
静默容器（180s 无输出）恢复后永远收不到「已恢复」；`notified` 一旦置真再无复位路径。→ 「已恢复连接」改为**三个触发点**：重连后首行日志 / Idle 结局 / 探针「不可达 → 可达」跃迁；三者均要求此前已提示过「正在重连」。

### #23 🟢 `log_status` 全仓无清空路径
切换容器后旧提示会留在新容器的日志面板上。→ `ui_apply::clear_log_panel`（文本 + 状态）/ `clear_log_status`（只清状态）两个落点；`AppState::ensure_follow` 返回「是否新建了流」，调用方据此清残留提示。

### #24 🟢 `RecoverNotice.sent` 无读取点
死字段。→ 删除 `RecoverNotice` 结构体，改为 `follow_logs(..., notify_recovered: bool)` 参数 + outcome 回传 `notified_recovered`。

### #25 🟢 缺存活性探针；`Msg::Containers(Err)` 不复位状态
容器是否还活着只能靠日志流自身推断；Docker 侧列表刷新失败时状态不回落。→ 新增 `probe_container` → `ContainerReach{Running,Stopped,Gone,Unreachable}`，探针结论是「重连还是结束」的**唯一**依据。

### #26 🟢 单测依赖真实 Docker 守护进程，且存在 180s 挂起路径
守护进程宕机时断言必红；走真实流的用例最长阻塞 180s。→ 单测全部改为纯函数断言（`follow_tail` / `classify_end` / `retry_delay` / `is_fatal` / `reach_from_state`），零 Docker 依赖。

### #27 🟢 文档措辞与实现不符 + CHANGELOG 缺条目
`docs/arch/threading.md` 描述的判定顺序与实现不符；`AGENTS.md` 消息表缺提示时机；`CHANGELOG.md` 缺 `1ce68d5`。→ 三处同步回填（见「文档回填」小节）。

### #28 🟢 `"Log stream reconnected"` 内联字面量
文案唯一定义处缺失，改文案要翻多个文件。→ 新增 `src/log_status.rs`，只保留 3 条 i18n key 常量（`NOTICE_INTERRUPTED` / `NOTICE_RECOVERED` / `NOTICE_STOPPED`）。

### #29 🟢 `follow_running: Arc<AtomicBool>` 跨流共享
旧流退出时会把新流的状态写脏，出现「刚重连就判定已停」的误报。→ 移除跨流共享标志，由探针权威判定；每条流只保留自己的 `active`。

## 修复对照

| 编号 | 级别 | 修复方式 | 落点 |
|---|---|---|---|
| #19 | 🟡 | 拆出纯类型模块 `log_types.rs` | `src/docker/log_types.rs` |
| #20 | 🟡 | `FollowAction` 三态 + Idle 不计预算 + 30s 保活兜底 | `src/follow_runtime.rs` |
| #21 | 🟡 | `End` 分支一律发「已停止」 | `src/follow_runtime.rs` |
| #22 | 🟡 | 「已恢复连接」三触发点 | `src/follow_runtime.rs` / `src/docker/logs.rs` |
| #23 | 🟢 | `clear_log_panel` / `clear_log_status` 双落点 | `src/ui_apply.rs` / `src/ui_state.rs` |
| #24 | 🟢 | 删 `RecoverNotice`，改参数 + 回传 | `src/docker/logs.rs` |
| #25 | 🟢 | `probe_container` + `ContainerReach` | `src/docker/container_detail.rs` |
| #26 | 🟢 | 单测改纯函数 | `src/follow_runtime/tests.rs` / `src/ui_update/tests.rs` |
| #27 | 🟢 | 文档回填（threading.md / AGENTS.md / README / architecture.md / CHANGELOG） | `docs/`、根 |
| #28 | 🟢 | `src/log_status.rs` 常量唯一定义处 | `src/log_status.rs` |
| #29 | 🟢 | 移除跨流 `follow_running`，探针权威判定 | `src/follow_runtime.rs` / `src/ui_state.rs` |

附加拆分（配合 R2）：`src/docker/logs.rs` 309 → 81 行；`src/docker/container_detail.rs` 171 行（inspect + probe）；`src/ui_state.rs` 275 行；`src/ui_update.rs` 232 行。

## MCP UI 验收（Slint MCP，release + `slint/mcp` feature）

窗口句柄 `{generation:"1", index:"1"}`，1280×760。全部通过截图肉眼核对状态条文字。

| 场景 | 触发方式 | 期望 | 实测 |
|---|---|---|---|
| 日志流正常 | 打开日志 Tab | 持续输出日志、无状态提示 | ✅ `tick` 持续滚动，状态位为空 |
| Clear 清空 | 点垃圾桶按钮 | 文本清空 + 状态清空 | ✅ 仅剩新到达的 1 行，底栏「日志已清空」 |
| 容器删除 | `docker rm -f` | 「日志流已停止」 | ✅ `r3-10-strip.png` 连续 5 帧均为「日志流已停止」 |
| Docker 整体不可达 | 掐断 unix-socket 代理 | 「日志流中断，正在重连…」 | ✅ `r3-13-crop.png` |
| Docker 恢复 | 恢复代理 | 「日志流已恢复连接」 | ✅ `r3-14-strip.png` 第 4 帧起转为「日志流已恢复连接」 |
| 退避序列 | stderr 观测 | 1s→2s→4s→8s→10s | ✅ 5 次重连依次为 1s/2s/4s/8s/10s |
| 切换容器 | 点另一张卡片 | 状态提示清除 | ✅ 抽屉换人，旧提示消失 |

> 验收手法：用一个转发到 `/var/run/docker.sock` 的 unix-socket 代理充当「Docker 出口」，只在**本应用实例**上掐断/恢复，
> 不触碰真实守护进程，用户其它容器全程不受影响。

## L4 洪峰复测

`probe_sustained.py`：96 并发向本机 HTTP 服务打满 25s，同时用 MCP 连续截图 55 次。

| 指标 | 第 2 轮 | 第 3 轮 |
|---|---|---|
| 洪峰请求数 | 43.5k ~ 70k | **72.5k** |
| MCP 截图最慢 | 0.117s ~ 0.452s | **0.087s** |
| 失败次数 | 0 | **0** |
| 进程 RSS | 50 ~ 52 MB | **47.9 MB** |

## 回归风险专项结论

| 风险项 | 结论 |
|---|---|
| 日志重复 | ✅ 无（重连且本流已推过日志时 `tail=0`） |
| 日志漏行 | ⚠️ 断流窗口日志不补回（需 `timestamps + since` 游标，见遗留 L1） |
| 串台 / 漏建流 / 重复建流 | ✅ 无（`stream_epoch` 过滤 + `ensure_follow` 幂等） |
| CPU 空转 | ✅ 无（Idle 不计失败预算，保活 30s 一次） |
| 僵尸任务 | ✅ 无（`cancel_rx` + `select!` 随时可退出） |
| R7 / R2 / R8 | R7 ✅；R2 ✅（最大文件 ui_state.rs 275 行）；R8 ✅（模块地图 + 消息表 + CHANGELOG 已回填） |

## 遗留（豁免）

- **L1 断流窗口日志不补回**：需 `timestamps + since` 游标推进式拉取，改动面涉及日志协议层，另开迭代处理；本轮仅在文档中标注为已知限制。
- **L2 daemon 级故障端到端复现**：本轮**已通过 unix-socket 代理完成端到端复现**（见「MCP UI 验收」），豁免作废。
- **L3 UI 未截图验收**：本轮**已完成截图验收**（6 个场景全部通过），豁免作废。
- **L4 `material-1.1.0/` 与 `vendor/` 排除 R2**：第三方引入物，不在本项目代码规范管辖范围，维持豁免。
- **L5 `{}` 均为 match 分支非空实现**：`grep '{}'` 命中项逐条核对均为 Rust `match` 的空分支（表达「无附加动作」），非空实现占位，维持豁免。
