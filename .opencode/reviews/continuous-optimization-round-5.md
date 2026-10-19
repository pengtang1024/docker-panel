# continuous-optimization 第 5 轮审查报告

审查范围：`git diff baseline-continuous-optimization..HEAD -- . ':(exclude).opencode'`
审查 HEAD：`a430afa`
reviewer：独立 general 子代理（只读）

## 结论

**需修改**。未发现 🔴 严重问题。
上轮 #13 已解决、#14 已解决、#15 部分解决。
新增 1 项 🟡（#16）与 3 项 🟢（#17~#19）；#17 与 #8 同类，可继续 waive。

## 验证证据

| 项 | 结果 |
|---|---|
| cargo test | 7 passed |
| fmt / clippy / check | ✔ |
| 正文行数（去注释） | ui_state 257 / ui_update 292 / consumer 218 / docker/logs 168 / models 191，均 ≤300 |
| R7 | 锁内只状态变更 + spawn；apply_ui 锁外 invoke |
| R8 | 文档基本同步 |

## 上轮问题核对

- **#13 已解决**：history_done 只在当前 stream epoch 首条日志或 Clear 时置 true；空历史/首流失败保持 false，可重试 tail=200；Clear 后不复活历史。测试覆盖切换容器/旧 epoch/Ended/无流 Clear/活动流 Clear。
- **#14 已解决**：文档明确 history_done 作用域为当前选中会话，重新选中（含同一容器）重置，Clear 持久性仅到下一次选中。
- **#15 部分解决**：活动流 Clear 已断言 stream_epoch/log_epoch/follow_id/logs/history_done；缺旧 epoch 行断言与 ensure_follow 接线。
- **#8**：继续 waive 可接受；#17 同类窄窗口建议一并 waive。

## 新问题

### #16 🟡 空缓冲 + 无活动流时 Clear 仍置 history_done=true，之后启动日志会被 tail=0 跳过

位置：`src/ui_state.rs:266-278`、`:281-289`、`:327-332`

触发链：
1. 停止容器无历史；打开日志 tail=200 EOF，无行，history_done=false，Ended 清 follow_id；
2. 点 Clear：logs 为空、follow_id 为空，但无条件置 history_done=true；
3. 启动容器写启动日志；再点日志 Tab，ensure_follow 走 tail=0，启动日志看不到，直到重新选中容器。

建议：`clear_log_buffer` 只在 `had_logs || !follow_id.is_empty()` 时置 true；空缓冲+无流保留 false。补对应测试。

### #17 🟢 LogStreamEnded 未 drain 时 ensure_follow 提前返回

与 #8 同类 ≤120ms 窄窗口，不丢数据/不串台，下一次点击可恢复。建议继续 waive。

### #18 🟢 测试仍缺活动流 Clear 的旧 epoch 断言与 ensure_follow 接线

- 活动流 Clear 后缺“旧 epoch 行必须丢弃”的断言；
- ensure_follow 的 tail 接线仍只测 helper。
- 注意 ui_update.rs 正文余量仅 8 行，建议拆测试文件。

### #19 🟢 ensure_follow 不防御 id 变化，存在潜在串台入口

当前调用链都先 clear_logs，无实际串台；建议加防御：follow_id 非空且 != id 时内部先 clear_logs。

## 遗留

- **#8 / #17**：waive（≤100/120ms 窄窗口，无串台/丢失，下一次点击恢复）。
- **#15/#18**：本轮补旧 epoch 断言；ensure_follow 接线测试受 R2 余量限制，拆测试模块后补。
- 收到首条历史后异常断流的 tail=0 限制已在 threading.md 记录（#10）。
