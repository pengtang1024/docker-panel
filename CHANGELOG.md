# 更新日志

本文件记录 Docker Panel 的重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.3.0] - 2026-10-20

### 新增

- **UI 全面重构（slint-ui-overhaul spec）**：主题系统（浅/深/auto 三态 + 22 语义色令牌）、顶栏+筛选栏+卡片网格+底栏+右侧抽屉布局、四种卡片组件、自定义 Tooltip、抽屉三 Tab（详情/日志/环境）、拉取/清理对话框。
- **本地化 i18n（gettext .po 按 locale 选择）**：`translations/zh_CN/LC_MESSAGES/docker-panel.po` 编译期嵌入二进制，`slint::select_bundled_translation` 按系统 locale（zh* → zh_CN）选择，未命中回落英文源串。修复 select 调用时机（必须在第一个组件创建之后，否则 `NoTranslationsBundled`）。
- **配置持久化**：`src/config.rs` 持久化 theme/view/filter/sort/search/log_follow 到用户配置目录 JSON（directories 定位 + 原子写），启动恢复、变更即存、退出落盘。
- **全局快捷键**：Ctrl+K 聚焦搜索、Ctrl+R 刷新、Ctrl+1~4 切换容器/镜像/网络/卷视图、Esc 关闭抽屉/弹窗。实现于 app.slint 根级 FocusScope（capture-key-pressed），启动时延时焦点引导保证快捷键零点击可用；Esc 关闭弹层后主动归还焦点。
- **网络/卷视图搜索过滤**：搜索词同时过滤网络（名称/driver/scope/ID）与卷（名称/driver/挂载点），搜索/筛选/排序回调即时重建列表（网络/卷不在 3s 定时刷新内）。

### 变更

- **日志流自愈**：`follow_logs` 返回 `LogStreamOutcome`，Docker 侧错误按 1s→2s→4s→8s→10s 退避自动重连（可取消；**连续失败预算 8 次耗尽后转 30s 低频保活，永不放弃跟随**）；运行中容器遇到 EOF 同样自愈重连，容器已停止/被删除则结束流并提示「日志流已停止」；超过 180s 无任何日志判定连接挂死并走**保活**分支（静默重连、不计失败预算、不打扰用户）；容器被删除（HTTP 404）等致命错误直接结束流，避免僵尸任务。**容器是否还活着一律由存活性探针 `probe_container` 判定**（`Running`/`Stopped`/`Gone`/`Unreachable`），不再用跨流共享的 running 标志。每条流持有独立存活标志，`ensure_follow` 依据「id 相同且流仍存活」判定幂等，消除 `LogStreamEnded` 未 drain 窗口内切 Tab 不建流的问题。日志流状态走独立通道（带 epoch 过滤），显示在日志面板工具栏，不占用底部状态栏；三态文案唯一定义在 `src/log_status.rs`：「日志流中断，正在重连…」/「日志流已恢复连接」/「日志流已停止」。
- **日志面板改为虚拟化行模型（卡顿根治）**：旧实现把整段日志塞进一个 `TextInput`，每来一行日志都要让软件渲染器重排 + 重绘全部缓冲行（实测 500 行时吃掉一个 CPU 核）。新实现是**虚拟化 `ListView`**（逐行 `LogLineRow` delegate，只实例化视口内的行），且行模型是日志窗口的**镜像**——用「全局行流位置」对齐，能追加就追加（`LogSyncPlan::Append`，只 `extend`，可见行不重排），窗口裁剪过前段才整体重建一次（`LogSyncPlan::Reset`）。日志窗口上限从 500 行提到 **2000 行**（分块裁剪，保留 1000 行），「复制日志」与面板所见一致。
- **日志投递改行批（`Msg::LogBatch`）**：`follow_logs` 不再逐行 `tx.send`（洪峰下每行唤醒一次 consumer 形成惊群，实测 CPU 240%+），改为攒批投递（满 1024 行或距首行 60ms 发一条），空闲超时改用**常驻 `Sleep`**（每行只重置截止时间，不再逐行新建 `timeout`），每行只做一次 UTF-8 校验 + 一次堆分配。`Msg::LogLine(u64, String)` → `Msg::LogBatch(u64, Vec<String>)`。
- **stats 采样改整轮打包（`Msg::StatsBatch`）**：6s 采样不再逐容器单发（卡片数值在 1~2 秒内分批阶梯跳动，感知为闪烁），整轮打包成一条消息，所有卡片同帧更新。`Msg::Stats` → `Msg::StatsBatch(Vec<(String, ContainerStatsSnapshot)>)`。
- **变化区域按列表独立标记**：`DrainResult` 的 `lists_changed` 拆成 `containers_changed / images_changed / networks_changed / volumes_changed`，一条 stats 只重建容器行，不再连带重建镜像/网络/卷（这些行没变还要逐行比对）。`apply_msg` 改为按值消费 `Msg`，日志批整批**移动**进缓冲（逐行 clone 在洪峰下直接打满 CPU）。
- **模块拆分（单文件 ≤300 行）**：main.rs → [src/consumer.rs]（Msg 泵 + 3s 刷新 interval）+ [src/bootstrap.rs]（6s stats 采样 + 首屏加载）；ui_state.rs → [src/detail_fmt/]（mod + container / image / netvol）+ [src/follow_runtime.rs]（日志流运行时）+ [src/ui_state/log_sync.rs]（日志窗口裁剪 + 行模型同步账）；卡片拆为 [ui/cards.slint] 网格容器 + card-container / card-image / card-netvol；行构建 → [src/ui_rows/]（mod + containers / images / netvol）；日志流拆为 [src/docker/logs.rs]（读流）+ [src/docker/log_types.rs]（结束原因/错误分类/退避常量）+ [src/docker/container_detail.rs]（inspect + 存活性探针）；新增 [src/render.rs]（CachedModel 差量同步）、[src/log_status.rs]（状态提示文案唯一定义处）、[src/callbacks_drawer.rs]、[src/callbacks_netvol.rs]、[ui/menu-layers.slint]（浮动菜单层）、[ui/drawer-log-row.slint]（日志行 delegate）。

### 修复

- **日志流自愈的静默缺口**：① 空闲超时（连接挂死）与真实错误共用失败预算，8 次后直接放弃跟随——改为 Idle 走独立保活分支，不计预算、永不放弃，预算耗尽转 30s 低频保活；② 容器被删除等致命错误零 UI 提示——非取消的结束一律提示「日志流已停止」；③ 「已恢复连接」只依赖首行日志且提示标志单向锁存——改为三个触发点（重连后首行日志 / Idle 结局 / 探针由不可达跃迁为可达）；④ 切换容器后旧流的状态提示残留在新容器面板——新增 `clear_log_panel` / `clear_log_status` 两个清除落点，`ensure_follow` 返回「是否新建流」驱动清除。
- **日志洪峰/高频 Msg 导致 UI 卡死**：consumer 改为单批最多 2048 条的有界 drain + 120ms UI 节流；日志洪峰只**追加**日志行模型，不再每收一条日志就重建容器/镜像/网络/卷/抽屉。锁持有时间与 UI invoke 次数均变为有界，点击/搜索不再被饿死。
- **日志行模型随缓冲整表错位**：旧环形缓冲每来一行就挤掉最旧一行，整表索引每行错位一次，Slint 的 ListView 每次都把可见行标记为脏并重新排版——这是卡顿主因。改为分块裁剪（攒够 1000 行才整体搬移一次）+ 追加式行模型，稳态下可见行完全不重排。
- **状态栏结果永不恢复**：复制日志 / 清空日志 / 复制字段值直接写状态栏后一直停在结果上（看起来像卡住）。新增 `ui_apply::status_toast`，2s 后自动恢复「就绪」；期间若被其它消息改过则不覆盖（避免旧定时器盖掉新结果）。
- **Tooltip 气泡与按钮脱节**：`Tip.btn_h` 写死 32px 且从不随悬停元素更新，翻转（下方空间不足时向下弹）时气泡与按钮之间出现空隙。改为由悬停元素写入真实高度（`IconBtn.size` / `Dot.dot_size`），气泡箭头重新贴住按钮下沿。
- **Docker 无响应导致数据“冻住”**：列表 / stats / inspect / 卷占用调用统一 10s 超时；3s 刷新增加 in-flight 去重，避免慢守护进程下任务无限叠加耗尽 tokio 工作线程。
- **关窗后进程不退出**：主题监听的 dconf 阻塞读移到独立 reader 线程，监控线程用 200ms `recv_timeout` 检查 shutdown；轮询回退路径同样 200ms 醒来，`theme-monitor` join 不再永久阻塞。
- **抽屉日志交互**：关闭滚动容器的左右拖拽平移；日志按 `char-wrap` 折行消除提前换行留白；自动跟随在内容高度变化时贴底，关闭跟随后视口保持不动；切容器时同步清空旧日志行并取消旧日志流，避免不同容器日志串台；工具条 Clear 以 `tail=0` 重启当前跟随流，清空前的排队日志不会回填。
- **旧日志回填/串台**：日志行携带 stream epoch，consumer 只接受当前流；clear/切容器后队列里的旧行直接丢弃。`apply_ui` 构建的日志行增量也带 buffer epoch 校验，避免已投递的 UI invoke 把旧行写回。日志流自然结束发送 `LogStreamEnded(epoch)`，仅当前流清空 `follow_id`。
- **日志续流重复**：流自然结束后缓冲仍在，`ensure_follow` 改用 `tail=0` 续流，避免切回日志 Tab 时重放最近 200 行；`history_done` 在收到首条日志，或在有缓冲/活动流时 Clear 后置位，空历史/首流失败仍可 `tail=200` 重试；空缓冲且无活动流时 Clear 不置位，避免跳过之后的启动日志。`LogStreamEnded` 同时释放 cancel sender。
- **consumer 更新延迟**：有待更新消息时按 120ms 节流窗口剩余时间等待，最坏 UI 应用延迟由约 200ms 收敛到约 120ms；无变化的批次（例如只有被丢弃的旧日志）不再触发 apply/invoke。
- **抽屉环境挂载布局**：挂载项按 chip 行渲染，端口/网络/挂载盒宽度与复制按钮对齐，修复右侧裁切。
- **抽屉详情代码盒键值布局**：环境 Tab 的 `KEY=value` 行原两段 Text 无 stretch 划分，键名被摊宽、长值溢出容器；改为键名自然宽、值吃满剩余宽度并自动换行。
- **抽屉日志 Tab 不拉流**：切到「日志」Tab 未建立 follow 日志流（仅切 Tab 不请求）。新增 `AppState::ensure_follow`（按 id 幂等建流，避免重复 follow），容器选中与 Tab 切换两条路径统一调用。
- **卡片「···」菜单点外部不关闭**：新增卡片区背景点击层（`CardsArea` 根 `TouchArea`）并让卡片 select 前置 `CardMenu.close()`，点击其它卡片或空白处均收起菜单。
- **拉取弹窗缺原型要素**：补齐副标题、`Image`/`Progress` 字段标签、输入框 placeholder（`nginx:latest`）、常显进度条与状态文本（`等待拉取…`）。
- **清理弹窗缺原型要素**：补齐副标题、四行资源图标（▢/▣/◇/▤）、各类型数量回填、深色勾选态复选框、头/脚分隔线。
- **排序菜单锚点偏移**：改为左对齐锚定排序按钮（`menu_x = 按钮 absolute-position.x`）、宽 120px。
- **图标字形 tofu**：日志工具栏/顶栏/卡片菜单的 `⏸`/`⧉`/`🗑` 等字形系统字体缺失（仅 Noto Color Emoji/Math 覆盖 → 显示豆腐块），换为系统可渲染字形（`‖`/`❐`/`⌫`）。
- **网络详情容器数英文回退**：Rust 侧 `trf("{0} containers")` 命中 po 的复数条目（键为 `msgid\0msgid_plural`）失败而回落英文；新增 `i18n::trfp`（复数翻译，对齐 Slint `@tr("sing" | "plural" % n)`），网络详情容器数现为「N 个容器」。
- **Esc 关闭弹层后焦点悬空**：条件渲染移除弹窗后其内部焦点丢失导致快捷键失效，Esc 分支末尾 `global-shortcuts.focus()` 归还焦点。
- **顶栏/底栏/Chip/Stat 垂直对齐**：Slint 1.18 HorizontalLayout 默认 `cross-axis-alignment: stretch` 只让 Text 撑满居中，有固定尺寸的 Rectangle/Dot/Path/IconBtn 贴顶。给 TopBar/FootBar 根、OvItem、Chip 基类、Stat 内部、容器卡片 `.c-res` 等 HorizontalLayout 统一加 `cross-axis-alignment: center`；同时去掉 VCenterText 的 `vertical-stretch: 1`（center 模式下无效）。
- **卡片底部内容被切割**：四张卡片的 VerticalLayout 设了 `height: parent.height - 16px` 固定约束 + `clip: true`。cross-axis:center 后子元素按 preferred-height 计算，最挤情况（ports 行 + compose 标签）恰满 73px 零余量被切。修复：① card_h 从 89px 增到 140px；② 四张卡片 VerticalLayout 全部去掉固定 height 改 preferred-height 自适应。装饰元素（状态条/分隔线）保留 `parent.height - Npx` 正确用法。
- **drawer.slint 编译阻断**：TextInput 非法属性 `min-height(100%)` → `height(100%)`（Slint 只允许 width/height/preferred-* 百分比）；Flickable/logbox/Rectangle 缺失 3 个闭合大括号；文件末尾多余 1 个闭合大括号。

### 移除

- 旧三栏布局（Sidebar + 内嵌 DetailPanel）由新布局与右侧 Drawer 取代。

## [0.2.3] - 2026-10-17

### 新增

- **右键上下文菜单**：容器行 / 镜像行右键弹出 ContextMenuArea，容器菜单含"查看详情/日志/启停/重启/删除"5 项，镜像菜单含"查看详情/删除"2 项。行尾按钮保留作为快捷操作。
- **表头点击排序**：容器列表可按服务名称 / 运行状态 / CPU% / 内存 4 列排序。点击当前排序列切换升/降序，点击不同列切换排序列。排序状态保持在 AppState 里，刷新不丢失。按状态/CPU/内存排序时自动关闭 Compose 分组视图。

### 修复

- **Slint padding 警告**：detail-panel / image-detail-panel / log-panel 中 Text 元素的 padding-top/bottom 改用 VerticalBox 包裹，release build 零警告。

## [0.2.2] - 2026-10-16

### 新增

- **镜像详情面板**：点击镜像行 → `inspect_image` → 右侧面板切到第 3 Tab（镜像），显示 RepoTag / 创建时间 / 作者 / 大小 / 平台 / Entrypoint / Cmd / 工作目录 / 暴露端口 / Volumes / Labels / Env 共 14 字段。
- **网络 Tab 真实容器计数**：从容器列表反推每个网络的连接容器数，零额外 API 调用，网络行右侧实时显示。

## [0.2.1] - 2026-10-16

### 新增

- **Stats 资源监控**：容器列表新增 **CPU% / 内存使用 / 网络流量** 三列。双采样算法（`compute_cpu_delta`）用 tokio 6s interval 并发 join_all 拉所有 running 容器 stats，首次采样前 6s 显示 0%。
- **网络 Tab + 卷 Tab**：主界面 Tab 栏从 2 个扩展到 **4 个**（容器/镜像/网络/卷），主骨架抽离为 `ui/list-area.slint`（TabBar + ListArea）。
- **Compose 分组**：容器行按 `compose_project` 排序 + 分组标题行渲染（📁 Compose Project: xxx），ListView delegate 内 `is_group_start` 属性触发分组行。
- **日志流面板**：右侧第二 Tab（LogPanel）用 bollard `follow_logs` Stream → `tx.send(Msg::LogLine)`，AppState.logs 环形缓冲 cap=500。
- **Labels section**：详情面板最后一节，`ContainerDetail.labels` 格式化为多行 `key=value` 字符串显示。
- **操作完成立即刷新**：ActionDone / PullDone / CleanupDone 消息到达后 consumer 线程在锁内直接 `s.refresh()`，不等 3s Timer 周期，容器启停/拉镜像/清理后列表秒级刷新。
- **优雅退出**：consumer 线程 `recv_timeout(200ms)` + `AtomicBool shutdown`，`ui.run()` 返回后设 flag → join consumer → state drop → Runtime drop 自动 shutdown。窗口关闭不再依赖 OS 回收。
- **Slint MCP debug_info**：build.rs 加 `with_debug_info(true)`，`SLINT_EMIT_DEBUG_INFO=1` 环境变量支持 Slint inspector。

### 架构调整

- **Slint Timer → tokio interval**：软件渲染器 + MCP 模式下 Slint Timer 不触发（连 debug eprintln 都没有），改为 `rt_h.spawn(async { tokio::time::interval(...); loop { tick().await; } })`。定时刷新 3s + Stats 6s 都走 tokio interval。
- **Stats 必须 spawn 在 runtime 里**：`state.rt.handle().clone()` 先从 Mutex 里 clone handle + tx + running ids，drop lock 后 spawn。Stats 写入 `state.stats_prev: HashMap<String, ContainerStatsSnapshot>` 双采样缓冲。
- **跨文件 global 不可见**：Slint 1.14 的 AppActions global 必须在 app.slint 里定义，其他 .slint 文件通过 component callback property 注入（`ListArea.container_click(id) => { AppActions.container_select(id); }`）。
- **drain_messages 统一处理 Stats**：每条 Msg::Stats 先写回 containers（累积更新），再一次性 match 更新列表/Detail/Logs——去掉之前的 `handle_one` 10 参数函数。

### 工程重构

- **app.slint 354 → 129 行**：TabBar + ListArea 抽离到 `ui/list-area.slint`（137 行），DetailPanel + LogPanel + 自绘 Tab 抽离到 `ui/detail-log-panel.slint`（89 行）。
- **ui_state.rs 314 → 253 行**：去掉参数爆炸的 `handle_one` 函数。
- **新增 7 个文件**：
  - `src/docker/stats.rs`（88 行）：fetch_stats + compute_cpu_delta
  - `src/docker/netvol.rs`（53 行）：list_networks + list_volumes（bollard 0.21 API 坑条）
  - `src/ui_rows.rs`（132 行）：4 种 Slint 行构建 + Compose 分组排序
  - `ui/list-area.slint`：4 Tab + 4 个条件渲染列表区（callback 注入）
  - `ui/network-views.slint`：NetworkRowDelegate + VolumeRowDelegate
  - `ui/detail-log-panel.slint`：右侧详情/日志面板（条件渲染 + 自绘 Tab 头）
  - `ui/actions.slint`：AppActions global 定义（独立文件供跨文件可见性测试备用）

### bollard 0.21 坑条修复

- `StatsOptionsBuilder` 路径：`bollard::query_parameters::StatsOptionsBuilder`（不是 `bollard::stats::*`）
- `list_networks`：不用 `<String>` 泛型，用 `Some(ListNetworksOptionsBuilder::default().build())`
- `list_volumes`：`list_volumes(None::<ListVolumesOptions>)` 需要 type annotation
- `ContainerStatsResponse.networks`：`Option<HashMap<String, ContainerNetworkStats>>`，`rx_bytes/tx_bytes` 是 **u64**（不是 `Option<u64>`）
- `docker.stats()` 返回 Stream：`.next().await` 取第一帧

## [0.2.0] - 2026-10-16

### 完全重写

- **UI 框架**：从 egui/eframe → **Slint 1.14** 原生 GUI。
- **渲染器**：wgpu/Vulkan → **纯软件渲染器**（`renderer-software` + `systemfonts`），零 GPU 依赖。
- **二进制体积**：40M+ → **12M**（`opt-level="z"` + LTO + strip + `panic="abort"`）。
- **资源占用**：idle RSS **33MB**，idle CPU **≈ 0%**（事件驱动，consumer 线程 `rx.recv()` 阻塞，替代旧 200ms 轮询）。

### 功能收敛（极简 Docker Desktop Base）

保留核心容器/镜像管理，移除 v0.1.x 的 Compose 分组、日志窗口、资源监控折线图、行内排序、多选批量、右键菜单等非"桌面管理基础版"功能。

### 新增

- **Pull 弹窗**：输入镜像名 → Docker API 实时进度 → 完成自动关闭。
- **Cleanup 弹窗**：3 复选框（stopped/dangling/unused）→ 执行 prune → 结果汇报。
- **Docker 连接状态角标**。
- **Slint MCP server**（`--features slint/mcp` + `SLINT_MCP_PORT=8080`）：直接从 Slint software renderer 拿 framebuffer 做截图，绕过 Wayland/GNOME 安全链。

### 工程

- 架构：**主线程** `ui.run()` + `slint::Timer::Repeated(3s)` → 定时触发 refresh；**consumer 线程** `rx.recv()` 阻塞 → 收到消息 → `invoke_from_event_loop` 用 `Weak<MainWindow>` 升级后更新 Slint Model；**tokio multi_thread** 执行 bollard API。
- **模块**：`src/main.rs`（入口）+ `src/callbacks.rs`（UI 回调绑定）+ `src/ui_state.rs`（AppState + drain_messages）+ `src/models.rs` + `src/slint_gen.rs`（include_modules! 集中入口）+ `src/docker/*`（bollard 封装）+ `ui/*.slint`（Slint 标记分 **5 文件**：app 组装层 + models 数据桥 + delegates 行视图 + panes 横向 strip + dialogs 弹窗）+ `build.rs`。
- **文件 ≤ 300 行**：所有 `.rs` 和 `.slint` 文件正文均 ≤ 300 行。原 app.slint 387 行 → 先拆 3 文件（app/delegates/dialogs）→ 再拆 5 文件（加 models/panes），最终 app.slint 只剩 ~148 行骨架（纯 import 组装 + AppActions global + MainWindow 壳）；Slint 文件全部放 `ui/` 目录；delegates.slint 通过 callback 属性注入代替 AppActions global 直依赖，避免跨文件 global 可见性 + Rust glob import 问题。
- **Packaging**：`Depends` 移除 `libgl1`（软件渲染不需要）。

### 移除

- vendor/egui-0.36.2 目录（整个 egui crate 及 eframe、wgpu 传递依赖）。
- src/app/、src/widgets/、src/utils/ 三个旧模块。
- assets/MaterialIcons-Regular.ttf（Slint software-renderer-systemfonts 用系统字体）。
- 200ms 后台轮询循环。

[0.3.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.3.0
[0.2.3]: https://github.com/pengtang1024/docker-panel/tree/v0.2.3
[0.2.2]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.2
[0.2.1]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.1
[0.2.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.2.0
[0.1.1]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.1
[0.1.0]: https://github.com/pengtang1024/docker-panel/releases/tag/0.1.0