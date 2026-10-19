# docker-panel 抽屉 UX 修复 — 实施计划

## Task 1: 验证当前 ScrollView 滚动是否工作（详情 tab）
- **Status**: `pending`
- **Priority**: high
- **Depends On**: None
- **Description**:
  - 给 Drawer 临时加自动打开 + 加载第一个容器详情（main.rs 调试线程）
  - 给 detail_scroll ScrollView 临时加 `background: red` border 或 background 确认它真的有非零渲染高度
  - 用 MCP `dispatch_pointer_scroll` 滚，取前后两张 screenshot 对比内容位置变化
  - 用 `get_element_tree` 确认 detail_scroll 的 size (width/height) > 0 且 content VerticalLayout 的 preferred-height > ScrollView height
  - 若滚动不工作，定位根因（可能需要改 drawer.slint 布局）并修复
- **Acceptance Criteria Addressed**: AC-3
- **Test Requirements**:
  - `rule` TR-1.1: `get_element_tree` 返回的 detail_scroll size.height > 100px；evidence: MCP element tree JSON
  - `rule` TR-1.2: 两张截图（滚动前 vs 滚动后）内容相对位置变化可见；evidence: MCP screenshot pair
- **Notes**: 之前 MCP `dispatch_pointer_scroll` 传 window handle 可能不生效，需要找到 ScrollView 元素 handle 或直接 point to Drawer 区域坐标再滚

## Task 2: 验证详情 / 环境 tab 折行是否工作
- **Status**: `pending`
- **Priority**: high
- **Depends On**: None
- **Description**:
  - detail tab: 找有长 sha256 容器 ID 或长 entrypoint 的容器，截图确认 value 折行
  - env tab: 切到 tab=2，截图确认长 env value 折行（`dispatch_key_event` 切 tab + screenshot）
  - 若折行不工作（wrap: word-wrap 未生效或 Slint 1.18 bug），调研替代方案（TextInput read-only 或硬值宽度）
- **Acceptance Criteria Addressed**: AC-1, AC-2
- **Test Requirements**:
  - `rule` TR-2.1: 详情 tab 截图中长 value 完整显示（无 elide 截断）；evidence: MCP screenshot
  - `rule` TR-2.2: 环境 tab 截图中长 env value 折行；evidence: MCP screenshot (tab=2)

## Task 3: 详情 tab 每行加复制按钮
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 1, Task 2
- **Description**:
  - drawer-detail-row.slint: kind=0 (kv行) 的值区水平排右侧加 IconBtn (icon_name="copy", icon_size=12px)；kind=2 (代码盒) 的代码盒右侧也加
  - DetailPair 加新 callback `copy_value()` — drawer.slint 里 for row[i] 循环注入 `copy_value => { root.value_copy(row.v); }`
  - drawer.slint Drawer global 加新 callback `value_copy(string)` — app.slint 注入到 AppActions
  - callbacks_drawer.rs 加 `on_value_copy` 绑定，用 arboard 复制 + 设置 status_text toast
  - 参考已有 `log_copy_click` 的 arboard + SharedString::from(...) + status_text toast 模式
- **Acceptance Criteria Addressed**: AC-5, AC-9
- **Test Requirements**:
  - `rule` TR-3.1: 点击复制 icon 后 Wayland 剪贴板包含正确 value 字符串；evidence: arboard::Clipboard::get_text()
  - `rule` TR-3.2: 底部状态栏出现 toast 文本；evidence: MCP screenshot 视觉确认
  - `rubric` TR-3.3: 复制 icon 的颜色/大小/间距与现有 theme 一致；scale 1-5; anchors 1=明显突兀/占位不当, 3=基本协调, 5=完美融合; threshold >= 4; evidence: MCP screenshot + theme.slint 对比

## Task 4: 环境 tab 每行加复制按钮
- **Status**: `pending`
- **Priority**: medium
- **Depends On**: Task 3
- **Description**:
  - drawer-detail-row.slint: kind=3 (env 变量) 的每行 name=value 对，在 value 左侧或右侧加复制 icon
  - 复制内容为完整 "KEY=VALUE" 字符串（与日志复制格式一致）
  - 复用 Task 3 的 value_copy callback 链
  - Drawer 环境 tab 滚动验证（Task 1 同一结构，理论上同步生效）
- **Acceptance Criteria Addressed**: AC-4, AC-5, AC-9
- **Test Requirements**:
  - `rule` TR-4.1: 点击后剪贴板包含完整 KEY=VALUE 字符串；evidence: arboard get_text()
  - `rule` TR-4.2: 环境 tab 滚动条按需显示 + 可滚到底；evidence: 对比截图 + element tree
  - `rubric` TR-4.3: 视觉一致性；scale 1-5; anchors/threshold 同 TR-3.3

## Task 5: 清理调试代码 + 质量门禁
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 1-4
- **Description**:
  - main.rs 调试线程（auto-open + request_detail）删除，`callbacks::bind_callbacks(&ui, &state); let _ = ui.run();` 恢复原样
  - 清理未使用的注释（drawer-detail-row.slint 顶部关于 Rust 预折行的注释已过时，更新为 "Slint wrap: word-wrap 原生折行"）
  - 跑四条质量门禁 + 废弃旧名残留扫描
  - 最终 MCP build + 三 tab 全截图
- **Acceptance Criteria Addressed**: AC-6, AC-7, AC-8
- **Test Requirements**:
  - `rule` TR-5.1: `cargo check --all-targets` 零 error；evidence: shell 输出
  - `rule` TR-5.2: `cargo clippy --all-targets -- -D warnings` 零 warning；evidence: shell 输出
  - `rule` TR-5.3: `cargo fmt --all -- --check` 零 diff；evidence: shell 输出
  - `rule` TR-5.4: `grep -riE 'd[v]kit' src/ ui/ Cargo.toml packaging/` 退出码 1（零匹配）；evidence: shell 输出
  - `rule` TR-5.5: 所有 .rs / .slint 文件正文 ≤ 300 行；evidence: `wc -l` + 人工审查
  - `rule` TR-5.6: 三 tab 最终截图：详情折行+复制可见+滚动条按需、环境折行+复制可见、日志不受影响；evidence: 三张 MCP screenshot

## Task 0: (前置) 建立验证基础设施
- **Status**: `pending`
- **Priority**: high
- **Depends On**: None
- **Description**:
  - 临时在 main.rs 恢复 auto-open 调试线程（5s 后选第一个容器 + request_detail + 打开 Drawer），**最后会删**
  - 确保 MCP release 版本能启动且 8080 端口可达
  - 确认有容器有长 ID / 有足够 env 变量用于截图验证
- **Acceptance Criteria Addressed**: （为 Task 1-2 提供可重复验证环境，不单独对应 AC）
- **Test Requirements**:
  - `rule` TR-0.1: `SLINT_MCP_PORT=8080 ./target/release/docker-panel` 启动后 8080 端口 LISTEN；evidence: `ss -tlnp | grep 8080`
  - `rule` TR-0.2: 5s 后 MCP screenshot 中 Drawer 详情 tab 有可见内容；evidence: MCP screenshot
