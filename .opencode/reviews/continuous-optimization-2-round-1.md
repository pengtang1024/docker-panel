# continuous-optimization-2 第 1 轮审查报告

审查范围：`git diff baseline-continuous-optimization-2..35ad4d8 -- . ':(exclude).opencode'`
审查提交：`8cdfcfd` `acdb1de` `74ba0b2` `35ad4d8`
reviewer：独立 general 子代理（只读）

## 结论

**需修改**（无 🔴 逻辑缺陷，但有 1 项红线违规 + 3 项设计缺口）

三条需求的实现核验：

| 需求 | 结论 | 依据 |
|---|---|---|
| Cancelled/Eof/Error 三态 + 1→2→4→8→10s 退避重连，Eof 不重连 | ✅ | `logs.rs` 三态判定准确；`retry_delay` 位移无溢出 |
| 每条流独立存活标志 + ensure_follow 幂等 | ✅ | 每流独立 `Arc<AtomicBool>`；`ensure_follow` 改判 `follow_id == id && follow_is_alive()` |
| 重连时改 tail=0 避免重复行 | ✅ | `sent_any` 跨重连累积；首轮 0 行保持原 tail，有行后改 0 |

重点核验：取消路径可靠（watch 在最后一个 Sender drop 时关闭）、epoch 复用安全、无重复流/无忙等、R7 锁纪律合规。

## 问题清单

### #1 🔴 R2 红线违规：`src/ui_state.rs` 正文 328 行 > 300

基线 264 行，本轮 +64 行顶破红线。`spawn_follow` 单函数 53 行承担 6 项职责。
建议：把日志流运行时拆成独立模块（如 `src/follow_runtime.rs`），AppState 只留字段与薄封装。

### #2 🟡 Error 分支无重试上限，且不区分 404 → 僵尸任务 + stderr 刷屏

容器被删除后 `docker.logs(id)` 永远 404，会每 10s 无限重试；`eprintln!` 每 10s 刷行污染 journal。
建议：识别不可恢复错误（404 / No such container）直接结束；或加最大尝试次数；`eprintln!` 降频。

### #3 🟡 错误只进 stderr，UI 端无任何提示

退避重连期间日志面板表现为「内容不动」，用户无法区分「容器没输出」与「流断了正在重连」。
建议：首次失败时通过既有通道给一次 toast / 状态文本。

### #4 🟡 Eof 被当作「容器停止」的代理，运行中容器断流不自愈

Docker 侧干净关闭也表现为 EOF；此时容器仍在运行，日志会静默停止更新，与「自愈」目标相悖。
建议：EOF 时判定容器是否仍在运行，仍运行则按退避重连；或在文档中明确为已知限制。

### #5 🟢 `follow_logs` 文档注释未同步返回值语义

签名已改为 `-> (LogStreamEnd, u64)`，注释仍只描述旧 cancel 语义。

### #6 🟢 `src/ui_update.rs:218` 注释过时

「流自然结束（容器停止/Docker 异常）」与新实现不符：Docker 异常已由内部重连消化。

### #7 🟢 `AGENTS.md` 消息表未同步

`LogStreamEnded` 行未提 `follow_active` 清理，也未说明「Docker 异常已由内部重连消化」。

### #8 🟢 `spawn_follow` 职责过载（与 #1 同源）

状态写入与异步重连运行时混在一个函数，可测性差。

## 回归风险专项结论

| 风险项 | 结论 |
|---|---|
| 日志重复 | ✅ 无 |
| 日志漏行 | ⚠️ 断流窗口日志不补回（已文档化，未恶化） |
| 串台 | ✅ 无（epoch 唯一递增 + clear_logs 重置 + id 防御） |
| 漏建流 | ✅ 无（ensure_follow 三分支均落到「一条活流 or 新建一条」） |
| CPU 空转 | ✅ 无（退避 ≥1s） |
| 僵尸任务 | ⚠️ 见 #2 |
| 旧流误标新流 | ✅ 无（每流独立 flag + epoch 保护） |

## 遗留

- **#2 / #3 / #4**：本轮修复。
- 断流窗口日志不补回需 `timestamps + since` 游标，保留为已知限制。
