# 更新日志

本文件记录 Docker Panel 的重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 0.4.3 之前的版本缺少第三方许可文件与 Slint 署名，对应的 GitHub Release 与 tag 已下架；
> 历史变更记录保留在此处，但不再提供下载。

## [Unreleased]

### 修复

- **菜单「删除」项 hover 无高亮**：`MenuItem` 的 danger 分支把 `Theme.err_bg` 当**着色层**用，但 `err_bg` 是设计成整块背景的暗红（`TextButton` 拿它做 `bg_base` 才对）——`StateLayer` 只叠 opacity 0.08，8% 叠在 card 上几乎无变化（实测 Δ≈+2/255），删除项 hover 完全看不见。danger 项改用 `Theme.err`（着色用亮红）做着色层，8% 叠加得明显红调（实测 Δ≈+17/+6/+5），双主题都可见且语义正确。

## [0.7.3] - 2026-10-29

### 修复

- **compose 下单容器「删除」失效（点了没反应）**：台账容器行「··· → 删除」实际调用的是 `volume_remove(容器id)`——`app.slint` 的 `card_remove` 只路由了镜像 / 网络，容器（`kind=0`）掉进 `else` 分支被当成**卷**去删，容器本体从未被移除，表现为「停止了也删不掉」。补上 `kind==0 → container_remove` 分支，单容器删除恢复 `force(true)` 语义（停止 / 运行中均可删）。分组带「删除全部」与多选批量删除走的是另一条正确链路，不受影响。
- **台账行 hover 高亮闪烁、乱跑**：行背景只绑 `touch.has-hover`，而三个操作按钮浮层声明在行热区之上——鼠标移到按钮上会把 hover 从行热区「抢走」，行高亮掉回普通色，按钮显形条件随之翻假又翻真、快速来回抖动；且 `act_show` 漏了日志按钮的 hover，停在中间按钮上时整排按钮会消失。新增覆盖整行（含按钮区）的稳定 `row_hover`，背景与按钮显形统一取它。

## [0.7.2] - 2026-10-28

### 新增

- **镜像总占用可视化**：底栏新增「镜像大小」项（全部镜像 size 之和，实测约 6.4G 量级），与容器「磁盘」列（可写层、独占口径）互补——此前全 UI 没有任何地方能回答「镜像占了多少磁盘」。
- **磁盘列口径 tooltip**：台账「磁盘」表头悬停显示口径说明（可写层独占、共享镜像层不计入），堵住「容器磁盘怎么这么小」的误读——该数值与 `docker ps -s` 的 SizeRw 逐字节一致，是刻意选择的不重复计数口径。

## [0.7.1] - 2026-10-28

### 性能

- **共享 Docker 客户端（连接池复用）**：此前每次 API 调用都 `connect_with_local_defaults()` 新建 hyper 客户端 + 独立连接池——每 3s 刷新建 4 个、每 6s stats 每个运行中容器再建 1 个（11 容器 ≈ 每分钟 110 个），用完即弃、零复用。`Docker` 实为 `Arc<Transport>`，改为 `OnceLock` 进程级单例 + 廉价 clone，所有调用共享同一批 Unix socket 连接；列表 / stats / inspect 高频路径全部受益，`docker/mod.rs` 的旧注释反模式一并修正。
- **`stats_prev` 快照表随容器删除回收**：此前只增不减，长期运行的宿主机上反复起停容器会无界泄漏内存；`Msg::Containers` 应用列表时按本轮存活集合 `retain` 清理。
- **StatsBatch 应用 O(批量×容器数) → O(批量+容器数)**：每 6s 一轮 stats 逐条 `iter().find()` 定位容器行；先建一次 id→下标映射再整轮应用，几百容器时每轮省一个数量级的线性扫描。

## [0.7.0] - 2026-10-28

### 新增

- **设置页**：顶栏新增齿轮入口，弹窗集中管理「外观」（跟随系统 / 浅色 / 深色三选）与「自动更新」三个开关（启动时检查 / 自动下载并安装 / **安装后自动重启**），全部持久化到 `config.json`；更新弹窗内的两个开关保留（与设置页同源）。
- **安装后自动重启**（默认开）：应用内更新安装成功后不再停留在旧进程等用户手点——3 秒提示倒计时后自动启动替换后的新版本并退出事件循环；想手动重启的可在设置里关掉。

## [0.6.4] - 2026-10-28

### 修复

- **更新说明首字符被裁半截**（如「已是最新版本」的「已」只剩右半）：Material ScrollView 的 Flickable 不强制约束子元素宽度，弹窗里的裸 `Text` 写 `width: parent.width` 被忽略、按文本自然宽度布局；自然宽超过视口（含 8px 滚动条槽）时，Flickable 对水平不滚动的内容做居中（负偏移），首字符左半被视口裁掉。改为显式绑定 `sv.visible_width`，与抽屉 / 列表场景（delegate 宽度被强制约束）不同，这是唯一一处裸 Text 直接做 Flickable 子元素的用法。

## [0.6.3] - 2026-10-28

### 修复

- **更新说明的排版**：Release 正文是 Markdown 并内嵌 HTML 徽标，此前直接当纯文本展示，`##`、`**`、反引号、`<a><img>` 全部裸露。新增 `update::markdown` 在写入 `UpdateInfo` 前统一转换：标题转小节、列表转圆点、加粗/行内代码/引用标记剥离、`[文本](链接)` 只留文本、图片与分隔线整行删除、changelog 版本头剥方括号、连续空行折叠。

## [0.6.2] - 2026-10-28

### 修复

- **「立即重启」失败**：dpkg 用同路径 rename 替换二进制后，运行中的进程读 `/proc/self/exe` 会得到 `路径 (deleted)`（旧 inode 已从目录消失），直接 spawn 必然失败——剥掉后缀后从同路径启动替换后的新版本；裸二进制自替换后的重启同理受益。
- **主按钮 hover 文字撞色**：`TextButton` 的 filled 变体 hover 时文字被涂成与背景同色（深墨对深墨），视觉上文字消失；hover 时改为保持浅色文字。
- **更新说明里长链接的排版**：release notes 是纯文本 `word-wrap` 渲染，`Full Changelog: https://…` 这类无空白长串会把整行撑出容器被裁切；每 32 个字符插入一个零宽空格作为断行点，长链接可以在任意位置折行。

## [0.6.1] - 2026-10-28

### 修复

- deb 桌面入口 `Exec` 改为绝对路径 `/usr/bin/docker-panel`：GNOME Shell 对已有条目 `Exec` 变更的缓存刷新并不可靠，绝对路径在缓存未刷新等异常场景下仍能定位可执行文件，避免点菜单弹「未在 $PATH 路径中找到」。

## [0.6.0] - 2026-10-28

### 新增

- **应用内自动更新**：启动 10s 后检查 GitHub Releases（24h 节流，可在更新弹窗关闭），发现新版本时顶栏更新图标显示徽标；弹窗展示版本对比与更新说明，支持一键下载、SHA-256 校验、安装并提示重启。deb 安装（`dpkg-query` 命中 `docker-panel`）走 `pkexec dpkg -i`；裸二进制安装走同目录临时文件 + `rename` 原子替换，不直接写运行中的文件。开发构建 / 非 Linux / 无匹配资产 / 目录不可写时只展示原因并引导到下载页，不做半途安装。新增「启动时检查更新」「自动下载并安装更新」两个持久化开关（默认均开启）。

### 工程

- **更新检查不引入 TLS 依赖**：HTTP 走系统 `curl`（Linux / macOS / Windows 10+ 自带），只新增 `sha2` 做校验和；`deny.toml` 白名单无需为数据许可开口子。发布流水线的 `checksums.txt` 同时覆盖 deb 与裸二进制，裸二进制在 `target/release` 目录内计算，保证条目名与实际资产名一致。
- **更新任务线程纪律**：网络、`pkexec`、文件 IO 全部放在 `tokio::spawn_blocking`；进度按 1% 节流投递 `Msg`，consumer 统一灌 UI；检查/安装共用互斥标志，重复点击不会产生并发任务。

## [0.5.0] - 2026-10-27

### 新增

- **Compose 分组带支持组级操作按钮**：分组带 hover 后显示与容器行同构的 3 个按钮（主操作 / 日志 / 更多），动作统一作用于组内全部容器 —— 主按钮按「是否全部运行中」在**启动全组 / 重启全组**间切换（只动需要动的成员）；「更多」菜单提供**停止全部 / 重启全部 / 合并日志 / 删除全部**。未 hover 时该位置仍显示成员数，不打断原有的扫读节奏。
- **分组合并日志**：点击分组带的日志按钮，组内每个**运行中**容器各建一条日志流，合并显示在同一面板，行首带 `[容器名]` 来源前缀；抽屉进入「只展示日志」模式（隐藏详情 / 环境 Tab，避免切到上一个容器的残留内容）。多流共享同一取消信号与 `stream_epoch`，任一条流结束不代表整组结束；某个成员停掉只结束它自己那一路。
- **组操作口径 = 当前筛选后的容器**（与台账可见行同源），搜索 / 状态筛选之外的容器不会被误伤；新增单测覆盖项目键 / 服务键的成员解析与分组带运行状态字段（`group_running` / `is_running`）。

## [0.4.4] - 2026-10-26

### 工程

- **下架不合规旧版本**：删除 0.1.0–0.4.2 的 GitHub Release 与 git tag，旧发行物不再提供下载；`CHANGELOG` 保留历史记录并移除失效外链。

## [0.4.3] - 2026-10-26

### 工程

- **许可证合规：明确 MIT 只覆盖自有代码**：Slint 1.18.1 按 `LicenseRef-Slint-Royalty-free-2.0` 使用（三选一中的非 copyleft 分支），并在 README 与 GitHub Release 页面补 “Made with Slint” 署名；新增 `LICENSING.md`（许可选择记录）、`THIRD-PARTY-NOTICES.md`（组件分组与许可索引）、`licenses/`（7 份许可全文）与 `packaging/debian/copyright`（DEP-5）。deb 现在安装 `/usr/share/doc/docker-panel/{copyright,LICENSE,LICENSING.md,THIRD-PARTY-NOTICES.md,licenses/}`，Release 附件同步附带 `LICENSE` / `LICENSING.md` / `THIRD-PARTY-NOTICES.md` / `licenses.tar.gz`；`Cargo.toml` 补 `license = "MIT"`。
- **删除未使用的 `vendor/` Slint 源码副本**：`[patch.crates-io]` 早在 516709f 已移除，两个目录（61 个文件）不再参与构建，却会让公开仓库在 MIT 名下分发 Slint 源码；删除后构建仍使用 crates.io 原版 crate。
- **许可证门禁**：新增 `deny.toml` 与 `.github/workflows/license.yml`，push / PR 跑 `cargo-deny check licenses`；allow-list 只放行 MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode 等宽松许可与 `LicenseRef-Slint-Royalty-free-2.0`，GPL / AGPL / LGPL / SSPL / BUSL / Commons Clause 一律失败。

## 0.4.2 - 2026-10-22

### 修复

- **台账微条让分组带与容器行「数字差一倍、条却一样长」**：微条此前按「视图内最大值归一」——分组带用「各分组总和的最大值」、容器行用「最大的单容器」，两者是**不同量纲**（个体 vs 聚合）却画在同一列、同一根 52px 轨道上。结果：分组带的汇总数字是组内最大成员的 2 倍（实测 live-games 组 3.8% vs 成员 live-nats 1.9%），条却都是满格 52px，完全看不出差别；而且只有一个 compose 分组时它自己就是「最大的分组」→ 分组带**恒为满格**，条形零信息量，成员条还被压掉一半分辨率。现改为与 `docker stats` 的两个百分比同义的**绝对占比**：CPU = 相对单核的百分比 / 100（1.0 = 占满一核），内存 = `mem_usage / mem_limit`（1.0 = 占满配额）——与容器旁的数字同源；**分组带的条 = 组内成员条之和** → 父子关系自洽，全列同量纲可比（实测分组带 CPU 条 5px / 内存 9px，成员条 1px，父子关系一眼可辨）。极小占比给 1px 可见下限（`MIN_BAR_RATIO`），补偿只在**成员层**做、分组带累加补偿后的成员条 → 「组条 = 成员条之和」严格成立。概览带的 CPU TOP4 同步改用该基准（240px 轨道下绝对占比本就可见，不加补偿，保留排名区分度）。

### 工程

- **微条基准从「视图内最大值归一」改为绝对占比**：随旧刻度一并删除 `group_totals::max_base`（各分组总和的最大值）与 `group_totals::ratio`（值 / 基准）；新增 `containers::bar_ratio`（docker 口径 + 最小可见补偿）与 `containers::mem_ratio`（usage / limit）作为唯一入口，`GroupTotals` 改为直接累加**补偿后**的成员条比例，概览带 `build_ledger_summary` 不再需要传入 `max_cpu`。相关单测重写为 `bars_use_docker_scale`（容器行 / 分组带的绝对占比与父子可加性）与 `tiny_ratios_keep_a_visible_floor_and_stay_additive`（可见下限 + 父子可加性）。

## 0.4.1 - 2026-10-21

### 修复

- **启动后 CPU / 内存要等近一分钟才有值**：CPU% 靠相邻两帧差值计算，而首帧采样时本地还没有前一帧 → `compute_cpu_delta` 只能返回 0.0%，于是「6s 采样周期 + 等下一个周期凑齐差值」= 启动后 12s 内 CPU 列全是 0.0%、概览带的 CPU TOP4 是一片空白、内存 / 网络列要空到 6s 才出现。现改用 Docker 同帧返回的 `precpu_stats`（非流式 stats 请求时守护进程会先采一帧 pre、等约 1s 再采当前帧，两帧一并返回）**兜底首帧差值** → 进程刚起就有真实 CPU%；同时采样循环**不再跳过首轮**，且首屏拿到容器列表后**立刻补采一轮** → 启动约 1s 内 CPU / 内存 / 网络全部就位。
- **底部状态栏的 CPU / 内存 / 磁盘汇总恒为 0**：`ui/app.slint` 的 `cpu_total` / `mem_total` / `disk_total` 三个属性早已绑定到 `FootBar`，但 Rust 侧**从未写过这三个属性** → 状态栏右下角长期显示 "0% CPU / 0MB 内存 / 0MB 磁盘"（与台账行里明明有值的数字自相矛盾）。现由 `consumer::apply_ui` 独占写入，口径 = 全部容器（与「N/N 容器」同源）：CPU / 内存只累加运行中容器，磁盘累加全部容器 —— 与分组带汇总同口径。
- **点「磁盘」表头排序毫无反应**：台账「磁盘」列显示的是 `size_rw`（可写层占用），排序却用了 `block_read + block_write` —— 这两个字段**从未被任何代码赋值，恒为 0** → 所有容器的排序键相等，稳定排序保持原序 → 点表头既不换顺序也不换箭头含义（而「磁盘」恰好还是默认排序字段）。现把排序键改为与显示值同源的 `size_rw`，并删掉恒 0 的死字段 `block_read` / `block_write`。

## 0.4.0 - 2026-10-21

### 新增

- **统一列表布局（无左侧栏）**：四个视图（容器 / 镜像 / 网络 / 卷）**全部是列表**，左侧栏整体撤掉、卡片网格整体作废。容器视图是**密集台账**——概览带（总数 / 运行中 / 已停止 / 需关注 + CPU 占用 TOP4）→ 表头 → 30px 虚拟化台账行；镜像 / 网络 / 卷是各自的**表格列表**（精简列 + 表头可点排序）：镜像 = `repo:tag` · 大小 · 创建时间，网络 = 名称 · driver · 子网，卷 = 名称 · 大小 · 挂载点。三个列表的排序**各自独立**（切视图回来保留）并持久化到 `config.json`，排序方向在表头用 ↑/↓ 标出（左对齐列的箭头跟在标签后，右对齐列的箭头用覆盖层放在列左端，都不破坏列起点对齐）。筛选栏的状态 chips / 排序菜单 / 多选按钮改为**只在容器视图出现**——其余三视图各有自己的表头排序，这些控件对它们无意义。列起点固定的承诺扩展到全部四个列表：表头与每一行逐列同 x 同宽（MCP `get_element_tree` 逐列比对验收）。
- **台账内联 Compose 树**：compose 层级不再由独立导航栏承载，改在**台账内部**用分组带表达——项目带 → 服务带 → 容器行（1~2 级，**只有多容器服务才出现服务带**，一服务一容器的常见项目下树只有 1 级）。分组带**通栏**（不参与列宽计算），带上有项目色点 + 容器数 + 展开箭头，点箭头或点分组名 = 展开 / 收起。分组是**分组不是筛选器**：排序字段只决定**组内**顺序，组间按组名稳定排，不再有「只看某项目」的范围状态。没有 Compose 归属的容器**直接在顶层**（depth=0，与分组名同缩进），不另设「独立容器」分组带。分组名落在 `44px + depth*18`，与**同层**容器行的名称列起点逐像素对齐——层级只靠缩进表达。展开状态存「**收起**的分组键」集合（`project:<名>` / `svc:<项目>/<服务>`，不在集合里 = 展开 → 新分组默认展开）并持久化，重启恢复。
- **台账微条的刻度**：行与分组是**两种量纲**（个体 / 聚合），共用一个刻度必然失真——分组的和 ≥ 组内任一成员（恒等式）→ 分组总和恒为本列最大值 → 分组带恒为满格（不携带信息），成员的条被压掉一半分辨率（最大的容器只剩 56%）。现改为**两套刻度**：行按「视图内最大的单容器」归一（满格 = 最大的容器），分组带按「各分组总和的最大值」归一（满格 = 最大的分组）。同一量纲内沿一列扫即可比大小；跨量纲（分组总和 vs 单容器）比大小看**数字**。
- **分组带显示资源汇总（成员之和）**：项目带 / 服务带不只显示名字与成员数，还显示该组的 **CPU / 内存 / 磁盘总和**，落在台账的 CPU / 内存 / 磁盘列上（与容器行**同 x**，逐列比对实测 `445 / 583 / 647`）。口径与容器行一致：CPU / 内存只累加**运行中**成员（停止的容器没有实时占用，行上显示「—」），磁盘累加**全部**成员（可写层占用是累计值，停止后依然成立）。汇总描述的是「这个组占了多少」→ **与组是否展开无关，收起时数字照旧**。微条也画，但**用分组自己的刻度**（各分组总和的最大值）——行与分组是两种量纲，共用一个刻度必然失真（见下条「台账微条的刻度」）。
- **计数口径统一为「可见容器集合」**：概览带与筛选 chips 都取实际渲染成行的那些容器，折叠一个分组后两个数字同步变小（实测 9 → 3），不会出现「台账 3 行而芯片写着全部 9」。
- **「需关注」判定**：容器处于死亡 / 暂停 / 重启中 / 正在移除状态，或已退出且退出码 ≠ 0（如 `Exited (137)`）时，行首显示琥珀色状态轨，概览带「需关注」计数同步 +1。判定只用 `list_containers` 已有的字段解析，不额外发 inspect 请求。

### 修复

- **卷列表的「大小」列恒为「—」、按大小排序无意义**：bollard 0.21 的 `SystemDataUsageResponse` 只建模了**废弃**的 `VolumeUsage` 键，而守护进程（API ≥1.25）的 `/system/df?type=volume` 返回的是 `Volumes` 数组 → `docker.df()` 解析出来的卷占用**恒空**。bollard 的低层入口（`process*` / `build_request`）都是 `pub(crate)`、没有公开的裸请求 API，现新增 `src/docker/raw_http.rs`：Unix socket + HTTP/1.1 + chunked 解码（阻塞 IO 放 `spawn_blocking`），自己读 `/system/df?type=volume` 的 `Volumes` 键。卷大小恢复显示（实测 3.1 KB / 902.2 MB / 1.1 GB 等），按大小排序同时恢复正常；`Size = -1`（非 local 驱动）仍按「不可用」跳过。
- **清理弹窗的「未使用的网络」实际删的是未使用镜像，「未使用的卷」完全没效果**：清理弹窗按原型口径有四个复选框（已停止的容器 / 悬空镜像 / 未使用的网络 / 未使用的卷），但 Rust 侧的 `docker::cleanup` 还是早期三开关版本的签名 `cleanup(stopped, dangling, unused)`，其中 `unused` 走的是 `prune_images(dangling=false)`（= `docker image prune -a`，会连带删掉**带 Tag** 的未使用镜像）。UI 重构成四复选框后，网络开关被错接到这个「未使用镜像」分支上，卷开关则被直接丢弃——**勾「未使用的网络」会删掉所有未被引用的镜像，勾「未使用的卷」则什么都不会发生**。现把 `cleanup` 改为 `cleanup(stopped, dangling, unused_net, unused_vol)` 四个开关，与弹窗一一对应：未使用网络走 `prune_networks`、未使用卷走 `prune_volumes`（不加 `all` 过滤器，与弹窗回填的「匿名卷」计数同口径，Docker 默认只 prune 匿名卷）；「所有未使用镜像」不再从弹窗暴露（原型口径里本就没有这一项）。新增 4 条 i18n 文案（未使用网络 / 未使用卷的清理结果与失败提示）。
- **按 CPU / 内存 / 磁盘排序时箭头方向与实际顺序相反**：`sort_containers` 这三个分支的比较器基线写成了降序，`sort_desc=true`（UI 上显示 ↓ 箭头）反而从小到大排，与「名称」列的方向语义完全相反。已把三处基线统一为自然升序，`sort_desc` 只负责反向——现在 ↓ = 大的在前，↑ = 小的在前，与箭头含义一致。台账把排序提到表头一级功能后，这个矛盾会直接暴露在主交互路径上。镜像 / 网络 / 卷三个列表的排序比较器同样按「自然升序基线 + desc 反向」写。
- **抽屉滚动条与列表滚动条不跟随应用主题**：抽屉详情 / 日志面板用的是内置 Material 3 组件库（`@material`）的 ScrollView / ListView，其色板由 `MaterialPalette` 提供；列表滚动条来自 std-widgets(fluent)。此前项目从未给 `Palette.color-scheme` 赋值，两者全部走 OS 主题——用户在应用里选「浅色」而系统是深色时，抽屉滚动条仍按深色渲染（浅底上的深色滑块几乎不可见），列表滚动条同理。现新增 `ui/material-theme.slint`，把项目设计令牌映射成完整的 `MaterialScheme`（53 个角色全覆盖）接入 `MaterialPalette.schemes`，并把 `Palette.color-scheme` 接到应用主题（auto / light / dark），两处滚动条及后续 M3 组件全部跟随应用主题。

### 工程

- **列表化改造触及 `ui/app.slint` / `src/ui_state.rs` 的 300 行上限**：`ui/app.slint` 拆掉侧栏时代遗留的空 `body-row` 包裹层、把 M3 色板应用抽成 `apply_palette()` 回调（去掉两份重复的 `changed` 块），正文回到 294 行；`src/ui_state.rs` 拆出 `src/ui_state/config_io.rs` 后正文 282 行。`ui/app.slint` 里新建的 `ui/main-area.slint` 承担主列表工作区（SubBar + BatchBar + ListArea），排序菜单浮层仍在 app.slint 保证渲染在最上层。
- **新增/删除的文件**（按职责拆分）：新增 `src/docker/raw_http.rs`（裸 HTTP 请求：Unix socket + chunked 解码，绕开 bollard 未建模的 `Volumes` 键）；新增 `src/ui_rows/group_totals.rs`（分组资源汇总：`GroupTotals` 累加组内成员的 CPU / 内存 / 磁盘 + 文本格式化；从 `compose_tree.rs` 拆出以守住 300 行上限，`compose_tree.rs` 正文 297 行）；新增 `ui/list-area.slint`（四个视图的列表宿主 + 空态 + 背景点击层）、`ui/image-list.slint` / `ui/network-list.slint` / `ui/volume-list.slint`（三个表格列表）、`ui/components-list.slint`（`ListCols` 列宽契约 + `ListHeadCell` 表头单元格）、`ui/ledger-tree-row.slint`（项目带 / 服务带 delegate）、`src/callbacks_sort.rs`（三个资源列表的排序回调）；删除 `ui/cards.slint` / `ui/card-image.slint` / `ui/card-netvol.slint` / `ui/compose-tree.slint`（卡片网格与侧栏树作废）、`src/compose_scope.rs`（`ComposeScope` 范围筛选整体作废，只留 `compose_collapsed`）。`src/ui_rows/compose_tree.rs` 改写为台账树行构建（`KIND_CONTAINER` / `KIND_PROJECT` / `KIND_SERVICE` + 分组键生成），其单测拆到 `src/ui_rows/compose_tree/tests.rs`（正文 159 行，与 `follow_runtime/tests.rs` 同一写法）以守住 300 行上限，`src/ui_rows/images.rs` / `netvol.rs` 补排序。
- **AGENTS.md 组件库描述与实际实现不符**：文档写「用了 Modal（inherits Modal），项目自己的 DialogShell 在 Modal 上盖了自定义 card」，但 `ui/components-dialog.slint` 实际是手搓的 `inherits Rectangle`，从未 import Material Modal。已按实际实现改写，并把 Material Modal 的坑表换成项目 DialogShell 的真实约束（TouchArea 的 100% 对 Rectangle 父级不可靠、slot 覆盖层级、`@children` 唯一性等），另补「M3 接项目主题」一节：全局属性赋值语句不合法、必须经本组件属性中转、赋值不会打断 `<=>` 绑定（依据 `i-slint-core-1.18.1` 源码）、M3 滑块自动隐藏时的像素验收姿势。同步更新 README / architecture.md / docs/arch/ui-interaction.md 的模块地图与文件行数。

## 0.3.4 - 2026-10-20

### 修复

- **排序状态与上下箭头偶发对不上**：排序状态有两个写入方——UI 回调改了 `AppState` 却没写 Slint 属性，只有 consumer 的 3s 心跳才回写，且回写的是锁内快照的旧值。用户改完排序后心跳写回的仍是旧方向，箭头最长滞后 3s，甚至与实际排序方向相反。现改为回调在 UI 线程同步写 `sort_field` / `sort_desc`（与筛选 / 视图同一套路），consumer 只刷数据不回写用户操作过的状态；另加 `AppState::rows_epoch`，consumer 应用行模型前比对 epoch，锁内建行、锁外应用期间排序条件被改时丢弃这批旧行。
- **点排序方向箭头没反应（只会打开菜单）**：排序按钮的外层 `TouchArea` 声明在 `HorizontalLayout` 之后，Slint 按声明顺序命中，外层热区盖住了布局内箭头自己的 `TouchArea`，`set_sort_dir()` 永远收不到点击。已把按钮热区移到布局之前（与 `Chip` / `ViewTab` 的 `StateArea` 写法一致），方向箭头恢复可点：点箭头直接翻转升/降序，点按钮其余部分仍然展开排序菜单。

## 0.3.3 - 2026-10-20

### 修复

- **历史 Release 页面的更新日志没能回填**：回填步骤用 `git tag --list` 列历史版本，但 `actions/checkout` 默认 shallow clone，仓库里只有触发流水线的那个 tag，取不到任何历史 tag，回填循环空转。改用 `gh release list`（API）列 release，与 clone 深度无关；immutable release 改不了则跳过并告警，不影响本次发布。

## 0.3.2 - 2026-10-20

### 修复

- **GitHub Release 页面没有更新日志**：工作流只用 `generate_release_notes`，而本仓库没有 PR，GitHub 自动生成的正文只有一行 `**Full Changelog**: <compare 链接>`，发出去的版本在发布页面上看不到任何更新日志。现在新增 `scripts/release-notes.sh` 从 `CHANGELOG.md` 抽出对应 `## [X.Y.Z]` 段落，由工作流以 `body_path` 传给发布步骤（`generate_release_notes` 保留，compare 链接追加在后面）；抽不到段落时流水线直接失败，避免发出没有更新日志的版本。另加「回填历史 Release 更新日志」步骤，把流水线修好之前发的 0.3.0 / 0.3.1 页面正文补齐（immutable release 改不了则跳过，不影响本次发布）。
- **发版流程漏掉版本更新日志这一步**：`docs/git-flow.md` 的发版流程写着「两处必须一致」却列了三项，hotfix 流程更是完全没有版本同步与 CHANGELOG 步骤。现已补齐三处同步的可执行动作、hotfix 的完整 6 步、门禁步骤与两次推送（tag 必须单独推），并新增 `scripts/check-version.sh` 核对 Cargo.toml / deb control / CHANGELOG 段落 / 底部链接 / 最新 tag 五项。

## 0.3.1 - 2026-10-20

### 修复

- **面板坞（Dock）显示「未识别程序」、图标变成通用占位图**：Slint 1.18 的 winit backend 只有在 `WindowInner::xdg_app_id()` 有值时才调用 `with_name(app_id, "")`，本项目从未设置过 → Wayland toplevel 完全没有 app_id（`WAYLAND_DEBUG` trace 里查不到 `xdg_toplevel.set_app_id`），GNOME Shell 无法把窗口匹配到 `docker-panel.desktop`。现在于 `MainWindow::new()` 之后、`ui.run()` 之前在 Wayland 下显式调用 `slint::set_xdg_app_id("docker-panel")`；X11 保持不设（winit 用 argv[0] 回退生成 `WM_CLASS`，显式设置会让 class 段变空）。

## 0.3.0 - 2026-10-20

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

- **Release 流水线构建失败**：GitHub runner 未预装 fontconfig/freetype 开发包，`yeslogic-fontconfig-sys` 的 build script 找不到 `fontconfig.pc` 而 panic（0.2.2、0.3.0 两次发版的流水线都停在 `cargo build`）。工作流 apt 列表补 `libfontconfig1-dev`（连带 `libfreetype-dev` → `zlib1g-dev`/`libbz2-dev`/`libpng-dev`/`libbrotli-dev`），并在 `docs/git-flow.md` 记录该依赖与本地复现方法。
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

## 0.2.3 - 2026-10-17

### 新增

- **右键上下文菜单**：容器行 / 镜像行右键弹出 ContextMenuArea，容器菜单含"查看详情/日志/启停/重启/删除"5 项，镜像菜单含"查看详情/删除"2 项。行尾按钮保留作为快捷操作。
- **表头点击排序**：容器列表可按服务名称 / 运行状态 / CPU% / 内存 4 列排序。点击当前排序列切换升/降序，点击不同列切换排序列。排序状态保持在 AppState 里，刷新不丢失。按状态/CPU/内存排序时自动关闭 Compose 分组视图。

### 修复

- **Slint padding 警告**：detail-panel / image-detail-panel / log-panel 中 Text 元素的 padding-top/bottom 改用 VerticalBox 包裹，release build 零警告。

## 0.2.2 - 2026-10-16

### 新增

- **镜像详情面板**：点击镜像行 → `inspect_image` → 右侧面板切到第 3 Tab（镜像），显示 RepoTag / 创建时间 / 作者 / 大小 / 平台 / Entrypoint / Cmd / 工作目录 / 暴露端口 / Volumes / Labels / Env 共 14 字段。
- **网络 Tab 真实容器计数**：从容器列表反推每个网络的连接容器数，零额外 API 调用，网络行右侧实时显示。

## 0.2.1 - 2026-10-16

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

## 0.2.0 - 2026-10-16

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

[0.7.3]: https://github.com/pengtang1024/docker-panel/compare/0.7.2...0.7.3
[0.7.2]: https://github.com/pengtang1024/docker-panel/compare/0.7.1...0.7.2
[0.7.1]: https://github.com/pengtang1024/docker-panel/compare/0.7.0...0.7.1
[0.7.0]: https://github.com/pengtang1024/docker-panel/compare/0.6.4...0.7.0
[0.6.4]: https://github.com/pengtang1024/docker-panel/compare/0.6.3...0.6.4
[0.6.3]: https://github.com/pengtang1024/docker-panel/compare/0.6.2...0.6.3
[0.6.2]: https://github.com/pengtang1024/docker-panel/compare/0.6.1...0.6.2
[0.6.1]: https://github.com/pengtang1024/docker-panel/compare/0.6.0...0.6.1
[0.6.0]: https://github.com/pengtang1024/docker-panel/compare/0.5.0...0.6.0
[0.5.0]: https://github.com/pengtang1024/docker-panel/compare/0.4.4...0.5.0
[0.4.4]: https://github.com/pengtang1024/docker-panel/compare/0.4.3...0.4.4
[0.4.3]: https://github.com/pengtang1024/docker-panel/releases/tag/0.4.3