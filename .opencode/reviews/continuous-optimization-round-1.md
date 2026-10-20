# continuous-optimization 第 1 轮审查报告

审查范围：`git diff baseline-continuous-optimization..HEAD -- . ':(exclude).opencode'`
审查基线：`baseline-continuous-optimization`
审查 HEAD：`2ef44ca`
reviewer：独立 general 子代理（只读）

## 结论

**需修改**。未发现 🔴 严重问题；有 2 项 🟡 一般问题，3 项 🟢 建议。

取证：diff 涉及 5 个源码文件；`cargo fmt --check`、`cargo check --all-targets`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test` 全部通过（4 个 poll_timeout 单测）。

关键路径核验：
- `poll_timeout` 的 pending 空/非空、`since_apply` 大于/小于窗口、到期返回 0 的边界正确；`recv_timeout(0)` 不会忙等；shutdown 最坏检测延迟仍 ≤100ms。
- epoch 在 `clear_logs` / `clear_log_buffer` / `ensure_follow` / `spawn_follow` 的状态变更均在 state 锁内完成，与 consumer 的 `apply_msg` 串行化；旧流 queued 日志、A→B→A、Clear 后新流首行均能正确按 epoch 接受/丢弃，无死锁、无重复流、无流泄漏。
- `last_refresh` 在 HEAD 全仓库零引用，删除彻底。
- R7 锁纪律、正文行数（consumer 210 / ui_state 230）合规。

## 问题清单

### #1 🟡 旧容器/清空前的日志仍可能通过已投递的 UI invoke 回填

位置：`src/consumer.rs:122-124`、`src/consumer.rs:145-166`、`src/callbacks_drawer.rs:71-78`、`src/callbacks_container.rs:113-120`

epoch 只能过滤 mpsc 队列里的 `Msg::LogLine`，管不到已经构建好的 `log_text` 字符串：

1. consumer 在 `apply_ui` 锁内构建旧 `log_text`，drop 锁，`invoke_from_event_loop` 投递闭包；
2. UI 线程在闭包执行前先处理 Clear / 切容器回调，清空 `AppState.logs` 并执行 `ui.set_log_text("")`；
3. 随后闭包执行，仍把第 1 步的旧文本 `set_log_text` 回去。

结果：state 已是空日志，但日志面板显示旧内容。Clear 后新流是 tail=0，若容器暂时没有新输出，旧文本会一直停留；切到无日志的新容器时同理。

建议：`apply_ui` 生成 `log_text` 时记录当时 epoch，invoke 闭包执行时与共享的 `Arc<AtomicU64>`（`clear_logs` / `spawn_follow` 在锁内同步更新）比较；不一致则跳过 `set_log_text`。不要在闭包里锁 `AppState`。

### #2 🟡 文档未同步，违反 R8 / S4 门禁

位置：
- `AGENTS.md:179`：消息表仍写 `LogLine(String)`
- `docs/arch/threading.md:76`、`:164`：仍写“工具条 Clear 只清缓冲、保留当前流”
- `docs/arch/threading.md:106`：消息表仍是 `LogLine(String)`
- `docs/arch/threading.md:158`、`:160`：示例仍是 `follow_logs(&id, tx, 200, cancel_rx)` / `Msg::LogLine(line)`
- `docs/arch/docker-bollard.md:11`：`tx.send(Msg::LogLine)` 未体现 epoch
- `CHANGELOG.md:25`：Unreleased 条目仍写“Clear 只清缓冲、保留当前跟随流”
- 次要：`AGENTS.md:152`、`README.md:100`、`docs/architecture.md:45`、`docs/arch/threading.md:34` 仍描述固定 `recv_timeout(100ms)`，未体现 `poll_timeout`

建议：同步 AGENTS 消息表、threading、README 架构概览、CHANGELOG；补充 Clear tail=0 + epoch、最坏 UI 延迟 200→120ms。

### #3 🟢 epoch 核心逻辑缺少测试

位置：`src/ui_update.rs:193-199`、`src/ui_state.rs:245-287`

建议：补 `AppState` 级别测试：旧 epoch 的 `Msg::LogLine` 经 `clear_logs` 后被丢弃，当前 epoch 行被接受。

### #4 🟢 `ensure_follow` 的流结束后 `follow_id` 长期不失效（既有问题）

位置：`src/ui_state.rs:266-272`、`:275-287`

流自身结束（容器停止导致 EOF、Docker 短暂异常）时不会清零 `follow_id`；此后 `ensure_follow` 因 `follow_id == id` 直接返回，日志不会重新建立。

建议：流退出时发送带 epoch 的结束消息；仅当 epoch 仍匹配时清空 `follow_id`。

### #5 🟢 被 epoch 丢弃的日志仍计入 `processed`，会触发无变化的 UI apply

位置：`src/consumer.rs:54`、`:81`、`src/ui_update.rs:193-199`

旧 epoch 行在 `apply_msg` 中 `processed += 1` 但不置 `logs_changed`；切流后若积压大量旧日志，consumer 仍会按节拍调用 `apply_ui` 并投递无变化的 invoke。

建议：`DrainResult` 增加 `has_ui_changes`，apply 判定改用它。

## 遗留

（本轮暂无 waived；#4 为既有问题，若本轮不修需在此记录理由）
