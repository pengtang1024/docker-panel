# Tasks: UI 彻底重构 v2

## 前置检查：Rust 侧 stats 数据是否真的灌入 Row

先跑一次 Rust 代码，确认 `ui_rows.rs` 的 compose_container_rows 有没有把 stats_prev 的 cpu_percent/mem_usage 写到 ContainerRow 里。截图显示 CPU/内存列是空的（但 RowButton 图标列是有值的）。如果是 Rust 侧漏了，先修 Rust 再改 UI。

---

## Task 1: Activity Bar 组件 + 主布局重排（高优先级）

**目标**：把 TabBar（顶部 36px 横向）替换成 Activity Bar（左侧 48px 竖向）

### 工作

1. **新建 `ui/activity-bar.slint`**（或放在 components.slint 里）
   - ActivityBar inherits Rectangle，width: 48px，height: 100%，background: Theme.bg-card
   - 4 个 ActivityItem（继承 Rectangle），竖直排列：
     - 图标 Text 居中（🐳/🖼/🌐/💾），font-size: Theme.text-base，color: inactive → text-muted / active → accent-soft
     - active 时：左侧 2px accent Rectangle（y:0, height:100%）+ 轻微 bg-hover 背景
     - hover 时：bg-hover 过渡
   - callback clicked(int index)
   - spacing: 0，每个 ActivityItem 高度 48px

2. **修改 `ui/app.slint` 主布局**
   - 移除 TabBar 组件（替换为 ActivityBar 竖排在左侧）
   - 三栏布局：
     ```
     TopBar (48px)
     ├─ ActivityBar (48px, 左) ─ ListArea (flex) ─ DetailLogPanel (320px, 右)
     ToolBar / BatchBar / StatusBar 在 ActivityBar + ListArea 下面
     ```
   - 删掉原 app.slint 里 TabBar 那一行 + Divider 行
   - ListArea 的 height 计算改回 `parent.height - TopBar - ToolBar - BatchBar - StatusBar`（去掉 TabBar 的 36px）
   - 把 ActivityBar 插在 ListArea 左侧，width 48px

3. **Rust 侧零改动**——ActivityBar 点击回调还是 active_view = 0/1/2/3，ListArea 条件渲染逻辑不变

### 验证

- **TR-1.1 (rule)**: grep -rn "TabBar" ui/ → 零残留
- **TR-1.2 (rule)**: 窗口左侧有 48px Activity Bar，4 个 icon 竖排
- **TR-1.3 (rule)**: 点击 Activity Bar icon 切换主视图（0-3），右侧 ListArea 内容正确切换
- **TR-1.4 (rubric)**: Activity Bar 视觉（阈值 ≥1.5）— icon 居中 + accent 左竖条 + hover 过渡
- **TR-1.5 (rule)**: cargo build 零 Slint compile error
- **TR-1.6 (rule)**: cargo clippy 零 warnings

---

## Task 2: 详情面板卡片化分组（高优先级）

**目标**：右侧 DetailPanel 从一坨 Text 改为 5 个独立 bg-card 卡片

### 工作

1. **修改 `ui/detail-panel.slint`**
   - 把 ScrollView 内部的 5 个 Section 从"SectionHeader + DetailRow 列表"包裹到 Rectangle { background: bg-card; border-radius: 8px; padding: space-2; } 里
   - 卡片与卡片之间的间距：space-3 (8px)
   - 卡片内先放 SectionHeader，再放 DetailRow 列表
   - SectionHeader 上方不加额外间距（卡片本身的 padding 已经提供）
   - 卡片内 DetailRow 的 padding-left: space-1 (4px)，行高 22px
   - SectionHeader 的 accent 竖条（4px wide × 12px tall）+ 加粗 accent 色标题保持

2. **DetailRow 组件微调**
   - label 列：Theme.text-faint，font-size: Theme.text-xs，min-width: 72px，right-aligned
   - value 列：Theme.text-normal，font-size: Theme.text-sm，overflow: elide

### 验证

- **TR-2.1 (rule)**: detail-panel.slint 里能找到 5 个 `Rectangle { background: Theme.bg-card; border-radius: 8px; }` 卡片
- **TR-2.2 (rule)**: 每个卡片顶部有 SectionHeader（accent 竖条 + accent 色加粗标题）
- **TR-2.3 (rubric)**: 卡片分组树层次感（阈值 ≥1.5）
- **TR-2.4 (rule)**: cargo build 零 Slint compile error

---

## Task 3: ToolBar icon-only 按钮行（中优先级）

**目标**：底部三个大 FlatButton（带文字）→ 三个 24×24 icon-only 按钮

### 工作

1. **新建 `IconButton` 组件**（components.slint）
   - inherits Rectangle，width: 24px，height: 24px，border-radius: 6px
   - hovered property + TouchArea
   - Text icon 居中（Theme.text-secondary 色 + Theme.text-sm 字号）
   - hover 时 background = Theme.bg-hover

2. **重写 `ui/panes.slint` ToolBar**
   - ToolBar 里三个 IconButton：⏹（停止全部）/ ⬇（拉取镜像）/ 🧹（清理资源）
   - spacing: space-3 (8px)
   - 整体居中（HorizontalLayout { } + 三个按钮 + HorizontalLayout { }）
   - 高度从 40px 改为 36px

3. **删除 ToolBar 原有的 FlatButton + 文字**

### 验证

- **TR-3.1 (rule)**: ToolBar 只有三个 IconButton，每个 width: 24px height: 24px
- **TR-3.2 (rule)**: hover 时 background = Theme.bg-hover，图标颜色 Theme.text-secondary → Theme.text-primary
- **TR-3.3 (rule)**: cargo build 零 Slint compile error

---

## Task 4: 弹窗完全自定义（中优先级）

**目标**：PullDialog + CleanupDialog 从原生控件改为 Slint 自绘组件

### 工作

1. **新建 `DialogShell` 组件**（components.slint）
   - inherits Rectangle，full screen（覆盖 MainWindow），background: rgba(0,0,0,0.4) 遮罩
   - 居中卡片（width: 400px，height: 自适应内容）
   - 卡片：bg-surface + border-radius: 10px
   - TouchArea 点击遮罩关闭（callback close()）
   - 内部插槽：Slot header / Slot content / Slot footer

2. **重写 PullDialog**
   - header：Text "拉取镜像" + 关闭按钮 ✕
   - content：Rectangle(bg-card + border-radius 6px) + LineEdit（placeholder "镜像名:tag"）
   - footer：FlatButton "取消" + FlatButton(accent 色) "拉取" 右对齐

3. **重写 CleanupDialog**
   - header：Text "清理未使用资源" + 关闭按钮 ✕
   - content：三个 CheckBox 每项外面包 Rectangle(bg-card + border-radius 6px + padding 12px)
   - footer：FlatButton "取消" + FlatButton(accent 色 + 删除红) "清理" 右对齐

4. **Rust 侧零改动**——callback 签名保持一致

### 验证

- **TR-4.1 (rule)**: 两个弹窗打开时有半透明遮罩
- **TR-4.2 (rule)**: 弹窗有标题区（Theme.text-lg + 加粗）+ footer 按钮区（右对齐）
- **TR-4.3 (rule)**: CleanupDialog CheckBox 每项独立卡片背景
- **TR-4.4 (rule)**: cargo build 零 Slint compile error

---

## Task 5: Theme 令牌清理（中优先级）

**目标**：所有 Slint 文件里硬编码的 color/font-size/background 清理干净

### 工作

1. 全局 grep 找所有硬编码：
   - `color: "#xxxxx"` → 替换为 Theme.text-*
   - `background: "#xxxxx"` → 替换为 Theme.bg-*
   - `font-size: 14px` / `15px` / `16px` → 替换为 Theme.text-sm / text-base / text-lg

2. 注意：Slint 1.18 不允许 `color: Theme.xxx` 作为 conditional（如 `color: running ? Theme.status-running : Theme.status-stopped`）——这个已经合法，保留。

### 验证

- **TR-5.1 (rule)**: grep -rn 'color:\s*"#' ui/*.slint → 零残留
- **TR-5.2 (rule)**: grep -rn 'background:\s*"#' ui/*.slint → 零残留
- **TR-5.3 (rule)**: grep -rn 'font-size:\s*[0-9]\+px' ui/*.slint → 零残留（除了 SortArrow 10px 和 SectionHeader accent 竖条相关）

---

## Task 6: 右侧内部 Tab（日志/镜像详情）样式 + 顶栏对齐（低优先级）

### 工作

1. **detail-log-panel.slint** 自绘 TabBar 样式重做
   - active Tab：accent 色 + 加粗 + 2px accent 下划线（精确宽度匹配文字）
   - inactive Tab：text-muted + 常规 + 透明下划线
   - Tab 高度 32px，文字 Theme.text-sm

2. **panes.slint TopBar** 四个元素高度对齐
   - Logo 胶囊（32px → 36px）
   - StatusChip（22px → 26px，或保持 22px 垂直居中）
   - SearchInput（当前 30px → 36px）
   - Refresh FlatButton（当前 28px → 32px）
   - 统一 height: 36px + vertical-alignment: center

### 验证

- **TR-6.1 (rule)**: 右侧内部 Tab active 有 accent 下划线 + accent 色加粗
- **TR-6.2 (rule)**: TopBar 四个元素高度一致 36px
- **TR-6.3 (rule)**: cargo build 零 Slint compile error

---

## Task 7: 完整质量门 + Commit

所有 Task 完成后统一过一遍：

- **TR-7.1 (rule)**: cargo fmt --all -- --check → 零 diff
- **TR-7.2 (rule)**: cargo clippy --all-targets --all-features -- -D warnings → 零 warnings
- **TR-7.3 (rule)**: grep -rn 'dvkit' src/ ui/ Cargo.toml packaging/ → 零残留
- **TR-7.4 (rule)**: wc -l 所有 src/*.rs + ui/*.slint → 全部 ≤ 300 行
- **TR-7.5 (rule)**: cargo build --release → Finished
- **TR-7.6 (rule)**: timeout 3 ./target/release/docker-panel → exit=124

---

## 依赖顺序

```
Task 1 (Activity Bar) → Task 3 (ToolBar) → Task 4 (Dialog)
                    → Task 2 (DetailPanel 卡片) → Task 6 (右侧 Tab + 顶栏)
                    → Task 5 (Theme 清理)     → Task 7 (质量门)
```

Task 1 是所有 UI 改动的前置——主布局先定下来，其他组件才有正确的容器关系。Task 2/3/4/5 可以并行（改不同文件），但 Task 6 依赖 Task 1 的布局稳定。Task 7 是所有任务收尾。
