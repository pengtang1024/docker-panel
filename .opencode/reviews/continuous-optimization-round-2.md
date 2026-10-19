# continuous-optimization 第 2 轮审查报告

审查范围：`git diff baseline-continuous-optimization..8e0be44 -- . ':(exclude).opencode'`
审查基线：`baseline-continuous-optimization`
审查 HEAD：`8e0be44`
reviewer：独立 general 子代理（只读）

## 结论

**需修改**。未发现 🔴 严重问题；有 2 项 🟡（#6、#7）和 1 项 🟢（#8）。
上轮 #2–#5 已解决；#1 主路径已解决，但 `clear_log_buffer` 无流分支仍有同类窗口（#6）。

## 验证证据

- `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo check --all-targets` 全部通过。
- `cargo test`：6 passed（4 个 poll_timeout + 2 个 AppState 级日志测试）。
- 正文行数：consumer 213 / ui_state 232 / ui_update 256 / docker/logs 168 / models 176，均 ≤300。
- `last_refresh` 全仓库零引用。
- 原子性/内存序：log_epoch 写点在 UI 线程且持 state 锁；consumer 读点在锁内；UI invoke 闭包只读 Arc<AtomicU64>，与写点同为主线程，AcqRel/Acquire 足够，无 ABA。
- Clear tail=0 主路径：`logs.clear()` 与 epoch 递增同锁，旧 epoch 队列行不会被接受。
- LogStreamEnded 不会误清空新流（取消路径先递增 epoch 再 drop sender）。
- R7 合规。

## 上轮问题核对

| # | 结论 | 说明 |
|---|------|------|
| #1 | 部分解决 | apply_ui 的 log_text 已带 epoch 校验；但 `clear_log_buffer` 在 `follow_id` 为空时提前返回、不递增 epoch，仍有回填窗口 → #6 |
| #2 | 已解决 | 文档已同步；仅 2 处轻微措辞残留 |
| #3 | 已解决 | 旧 epoch 丢弃/当前 epoch 接受/Ended 分支有测试；缺 clear_log_buffer 分支 |
| #4 | 已解决 | spawn_follow 结束发 Msg::LogStreamEnded(epoch)，consumer 仅当前 epoch 清空 follow_id；无泄漏；副作用见 #7 |
| #5 | 已解决 | has_ui_changes 覆盖全部字段；无变化批次不再 apply_ui |

## 新发现问题

### #6 🟡 Clear 在“流已结束”状态下不递增 epoch，旧 log_text invoke 仍可回填

位置：`src/ui_state.rs:258-265`、`src/callbacks_drawer.rs:71-78`

触发链：
1. 日志流自然结束，`LogStreamEnded` 只清 `follow_id`，`logs` 保留；
2. consumer 已构建旧 `log_text` 并投递 invoke；
3. UI 先执行 Clear：`clear_log_buffer` 因 `follow_id` 为空直接返回，epoch 不变，随后 `set_log_text("")`；
4. 旧 invoke 执行时 epoch 仍相等，旧文本回填；无活动流时永久停留。

建议：`clear_log_buffer` 无论 `follow_id` 是否为空都先递增 epoch；补无 Docker 依赖的单测。

### #7 🟡 LogStreamEnded 后重新切日志 Tab 会以 tail=200 重放，重复追加最近 200 行

位置：`src/ui_update.rs:215-220`、`src/ui_state.rs:268-273`、`:276-293`、`src/callbacks_drawer.rs:24-29`、`src/docker/logs.rs:169`

触发链：
1. 已停止容器：follow=true 返回历史日志后 EOF，Ended 清 follow_id，logs 保留；
2. 切到详情再切回日志：ensure_follow 固定 tail=200，Docker 重放最近 200 行 append 到已有缓冲；
3. 反复切 Tab 反复追加，最终 500 行缓冲被重复内容填满。

建议：同容器续流用 tail=0，且不要递增 buffer epoch；可按 `logs.is_empty()` 选择 200/0，或记录“同容器可续流”标志。若选择清空重拉，则清 logs + 递增 epoch。

### #8 🟢 结束消息尚在队列时 ensure_follow 可能空转

位置：`src/ui_state.rs:268-273`、`src/ui_update.rs:215-220`

流已结束、`LogStreamEnded(E)` 已入队但 consumer 未 drain（≤100ms）。此时切日志 Tab，`ensure_follow` 见 `follow_id == id` 直接返回；随后 Ended 清空 follow_id，本次切 Tab 实际未建流，需再切一次。

建议：用共享“流存活”标志/generation 判断，或接受为已知窄竞态并记录。

## 非阻塞观察

- `docs/arch/docker-bollard.md:11` 仍写死 `.tail("200")`；实际 tail 是参数（Clear 为 0），建议改为 `tail(tail_lines)`。
- `docs/arch/threading.md:175` “递增 epoch、drop 旧 sender”仅在 follow_id 非空时成立；修 #6 时澄清。
- `ensure_follow`/`spawn_follow` 无条件递增 epoch，在续流场景会作废仍有效的待应用文本；按 #7 改 tail=0 时需同步处理。
- #5 的 `pending.processed` 只增不减不造成内存/行为问题。

## 遗留

（待修复 #6 #7；#8 拟 waive，理由：窄竞态 ≤100ms，需额外流存活标志，当前收益低，会在报告/代码注释中记录）
