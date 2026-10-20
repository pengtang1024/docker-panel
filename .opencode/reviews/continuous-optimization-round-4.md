# continuous-optimization 第 4 轮审查报告

审查范围：`git diff baseline-continuous-optimization..HEAD -- . ':(exclude).opencode'`
审查 HEAD：`66fa984`
reviewer：独立 general 子代理（只读）

## 结论

**需修改**。未发现 🔴 严重问题。
上轮 #9 已解决，但引入新的日志丢失窗口（#13 🟡）；#10/#11 已解决；#12 部分解决；#8 继续 waive。

## 验证证据

| 项 | 结果 |
|---|---|
| cargo test | 7 passed |
| fmt / clippy / check | ✔ |
| 正文行数 | ui_state 255 / ui_update 275 / consumer 213 / docker/logs 168 / models 176，均 ≤300 |
| 废弃旧名新残留 | 无 |
| R7 | 锁内只状态变更 + spawn/clone；apply_ui 锁外 invoke |
| R8 | 文档基本同步 |

## 上轮问题核对

- **#9 已解决**：history_done 初始化/首次 tail=200/clear_logs 重置/clear_log_buffer 保留/自然结束/A→B→A 路径核对通过。
- **#10 已解决（文档）**：tail=0 断流窗口限制已记录。
- **#11 已解决**：tail 变量、Clear 无流分支、Ended 释放 sender、stream epoch 注释均同步；仅函数头 `Msg::LogLine` 未带 epoch（顺手修复）。
- **#12 部分解决**：Ended 释放 sender、Clear 保留 history、切容器重置已有断言；缺活动流 Clear 与 ensure_follow 接线测试。
- **#8 waive 可接受**。

## 新问题

### #13 🟡 history_done 在请求时置位，空历史/首流失败后 tail=0 可能永久跳过启动日志

位置：`src/ui_state.rs:282-287`、`src/ui_update.rs:208-213`、`src/callbacks_drawer.rs:24-29`

1. 首流失败：tail=200 尚未收到行就失败，history_done 已 true；之后 tail=0，历史不再补拉。
2. 空历史 + 容器启动：停止容器无历史，打开日志 tail=200 EOF 后 history_done=true；启动容器写日志，再点日志 Tab 走 tail=0，启动日志看不到，直到重新选中容器。

修复前 `logs.is_empty()` 会重拉，这属于行为变化。建议把 `history_done=true` 移到“收到当前 epoch 首条日志”，并在 `clear_log_buffer` 显式置 true。

### #14 🟢 history_done 与选中操作隐式绑定，Clear 作用域未文档化

同一容器重新点击卡片/日志按钮会 clear_logs，history_done 重置，历史会重新拉取。可解释为“重新选中=新日志会话”，但文档只写切容器重置。建议明确作用域：Clear 的持久性仅到下一次选中。

### #15 🟢 测试仍缺活动流 Clear 与 ensure_follow 集成断言

- 未覆盖 clear_log_buffer 的 follow_id 非空分支：stream_epoch +1、log_epoch +1、follow_id 保持、旧 epoch 行丢弃；
- ensure_follow 的 history_done 接线只测 helper；
- 若按 #13 调整，补“首行到达才置 history_done”与“首流失败可重试”。

## 遗留

- **#8**：继续 waive（≤100ms 窄窗口）。
- **#10**：文档记录；建议措辞覆盖自然 EOF/重启/空历史（#13 修复后空历史可重试）。
- **#13**：本轮实际修改。
