# continuous-optimization 第 3 轮审查报告

审查范围：`git diff baseline-continuous-optimization..HEAD -- . ':(exclude).opencode'`
审查 HEAD：`d5fc9dc`（修复提交 `78f393e`、`0e82462`，文档 `c7f8c36`、`814eedc`）
reviewer：独立 general 子代理（只读）

## 结论

**需修改**。未发现 🔴 严重问题。
上轮 #6、#7 已解决；#8 waive 可接受。
新增 1 项 🟡（#9）与 3 项 🟢（#10 #11 #12）。

## 验证证据

| 项 | 结果 |
|---|---|
| fmt / clippy / check | ✔ |
| cargo test | 7 passed |
| 正文行数（去空行/注释） | ui_state 253 / ui_update 268 / consumer 218 / docker/logs 168 / models 191，均 ≤300 |
| 废弃旧名新残留 | 无 |
| R7 | 锁内只状态变更 + spawn/clone；apply_ui 锁外闭包只读 Arc<AtomicU64> |

## 上轮问题核对

- **#6 已解决**：`clear_log_buffer` 在 `follow_id.is_empty()` 提前返回前递增 `log_epoch`；旧 apply invoke 比较失败。测试断言无流分支 epoch +1。
- **#7 已解决**：`stream_epoch` 过滤日志行/Ended，`log_epoch` 只做 UI 文本过期；`spawn_follow` 不推进 buffer epoch；`ensure_follow` 缓冲非空时 tail=0。关键路径矩阵（首次 A / A→B / B→A / 结束后续流 / Clear 有流）均正确。
- **#8 waive 可接受**：≤100ms 窗口，仅本次切 Tab 空转，无串台/丢失/崩溃；当前字段无法表达“已结束但 Ended 未 drain”。可选后继：每流独立 `Arc<AtomicBool>`。

## 新问题

### #9 🟡 Clear 在“流已结束”状态下清空不持久，切回日志 Tab 会重新拉取 200 行历史

位置：`src/ui_state.rs:261-281`、`src/callbacks_drawer.rs:24-31`

触发链：
1. 打开停止容器日志：tail=200 拉历史后 EOF，Ended 清 follow_id，缓冲保留；
2. 用户 Clear：清缓冲、递增 log_epoch，因 follow_id 为空 return；
3. 切到详情再切回日志：`ensure_follow` 见 `logs.is_empty()` → tail=200 → 清掉的历史全部复活。

与文档“只保留清空之后的新日志”不一致。建议：用 `history_done` 标志替代 `logs.is_empty()`；`clear_logs` 重置，`clear_log_buffer` 不重置；`ensure_follow` 按 `history_done` 选 200/0。

### #10 🟢 tail=0 续流在异常断流时会丢失断流期间日志

位置：`src/ui_state.rs:279`、`src/docker/logs.rs:174-191`

流因 Err 退出而容器仍写入时，tail=0 从当前时刻接续，断流窗口日志永久丢失；自然 EOF 无影响。建议在 `docs/arch/threading.md` 记录为已知限制；补齐需 timestamps + since 游标。

### #11 🟢 文档/注释与代码细节仍不完全一致

- `docs/arch/threading.md:170` 示例写死 `200`，实际 tail 是变量；
- `docs/arch/threading.md:176` “递增 log_epoch、drop 旧 sender，并以 tail=0 重启”仅在活动流时成立，需补无流分支；
- `CHANGELOG.md:26` 未提 Ended 同时释放 cancel sender；
- `src/docker/logs.rs:151` 注释“当前 epoch”应为“当前 stream epoch”；
- 测试名 `stale_log_epoch_is_dropped_after_clear` 实际断言 stream_epoch，建议改名。

### #12 🟢 测试覆盖缺口

- 未断言 `LogStreamEnded(当前 epoch)` 会清空 `follow_cancel`；
- 未覆盖 clear_log_buffer 活动流分支（stream_epoch +1、follow_id 保持）；
- `ensure_follow` tail 选择只测 helper，未测 #9 的 history_done。

## 遗留

- **#8**：waive（理由见上，≤100ms 窄竞态）。
- **#10**：修复 #9 时在文档记录为已知限制（不补 since 游标）。
