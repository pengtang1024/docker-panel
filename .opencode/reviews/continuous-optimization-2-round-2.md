# continuous-optimization-2 第 2 轮审查报告

审查范围：`git diff baseline-continuous-optimization-2..a8afbd9 -- . ':(exclude).opencode'`
审查 HEAD：a8afbd9（审查期间追加了 8bab054 / a8afbd9，本报告按当时实际代码审查）
reviewer：独立 general 子代理（只读）

## 结论

**需修改**（无 🔴 逻辑缺陷、无 R2 红线违规；上轮 #1~#8 全部闭环，但新增 4 项 🟡 与 6 项 🟢）

上轮核对：#1 已解决（ui_state.rs 正文 292 行，新增 `src/follow_runtime.rs` 单一职责）；#2 已解决（不可恢复识别可靠、上限 8 次≈55s、stderr 降频）；#3 已解决（`Msg::LogStreamStatus` 真正到达 UI，i18n 三条齐全）；#4 已解决（`follow_running` 判定可靠）；#5/#6/#7 部分解决；#8 已解决。

## 新问题

### #9 🟡 「已恢复连接」提示发错时机
`follow_logs` 只在流结束时返回 → 重连成功且流保持打开时永远不发消息，底栏一直停在「正在重连」；流结束时发「已恢复」又与事实相反。建议在重连成功那一刻发。

### #10 🟡 `follow_running` 陈旧窗口
仅在 `Msg::Containers` 成功时刷新，`follow_id` 为空时不复位 → 切到已停止容器时可能沿用上一容器的 `true`，触发 55s 空转重连与误报。建议 `ensure_follow` 现场计算 + Ended 复位。

### #11 🟡 日志流无超时 → 挂死流无法自愈
bollard 的 client_timeout 只覆盖响应头，不覆盖流式 body 读取；守护进程接受连接后不再推数据 → `stream.next()` 永久挂起 → 任务永不返回 → 无任何自愈路径。建议加 idle 超时。

### #12 🟡 单测依赖 Docker 守护进程
daemon 宕机时连接错误为 `Connection refused` → 判为可恢复 → 退避中仍存活 → 断言 `!follow_is_alive()` 失败，`cargo test` 必红。建议改为纯函数断言。

### #13 🟢 R8：新模块未进任何模块地图
`src/follow_runtime.rs` 在 README / architecture.md / AGENTS.md 三处均缺失。

### #14 🟢 文档/注释与实现不一致
AGENTS.md 消息表称「Docker 异常不会产生 Ended」（现会）；规范 #13 未提运行中 EOF 与重试上限；`ui_update.rs` 注释漏两种结束原因。

### #15 🟢 状态栏单槽
`status_text` 单槽后写覆盖；操作 toast 的 2s「Ready」定时器会抹掉日志流提示，反之亦然。建议日志流提示走独立通道。

### #16 🟢 `LogStreamStatus` 无 epoch
旧流的提示会落到新容器。建议带 epoch 并按当前 `stream_epoch` 过滤。

### #17 🟢 不可恢复判定靠字符串子串 + 与 classify_end 逻辑重复
`"404"`/`"not found"` 过宽（`Socket not found` 会误判）；bollard 结构化错误被拍平丢失 status_code；内联不可恢复分支与 `classify_end` 重复。

### #18 🟢 冗余测试 + `spawn_follow` 冗余参数
`follow_tail` 测试重复；`tx` 与 `tx_end` 是同一 Sender 的两次 clone。

## 回归风险专项结论

| 风险项 | 结论 |
|---|---|
| 日志重复 | ✅ 无 |
| 日志漏行 | ⚠️ 断流窗口不补回（已文档化） |
| 串台 / 漏建流 / 重复建流 | ✅ 无 |
| CPU 空转 | ✅ 无 |
| 僵尸任务 | ⚠️ 见 #11（挂死流） |
| R7 / R2 / R8 | R7 ✅；R2 ✅（ui_state 292 / follow_runtime 136，余量偏小）；R8 ⚠️ 见 #13/#14 |

## 遗留

- **#9~#12 / #13~#18**：已在 `23c2d22` 修复（复审见下一轮报告）。
- 断流窗口日志不补回需 `timestamps + since` 游标，保留为已知限制。
