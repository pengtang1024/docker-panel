# Docker Panel 架构设计（总纲）

v0.2.0 重写版：Slint 原生 GUI + 纯软件渲染器 + 三线程消息架构。
本文档描述当前实际代码状态，随仓库版本管理。按 R2（正文 >300 行必须拆分）拆分为**总纲 + 三个主题文档**。

写代码时必须按精确版本的 API 写，禁止凭旧版本记忆。
更完整的版本特有 API 限制清单：Slint 见仓库内 `AGENTS.md`，bollard 见 [arch/docker-bollard.md](arch/docker-bollard.md)。

## 0. 主题文档导航

| 文档 | 职责（权威源） |
|------|----------------|
| 本文件（总纲） | 技术栈、模块地图与依赖方向、文件规范 |
| [arch/threading.md](arch/threading.md) | 线程模型、消息流（Msg 变体 + 流水线）、Rust 回调绑定铁律、Rust 拆分原则 |
| [arch/ui-interaction.md](arch/ui-interaction.md) | Slint UI 模块交互（交互总图 + 4 种交互模式 + 文件清单）、Slint 拆分原则 |
| [arch/docker-bollard.md](arch/docker-bollard.md) | Docker 接入约定、bollard 0.21 易踩坑速查（**唯一权威源**） |

## 1. 技术栈（精确版本）

| 组件 | 精确版本 | Feature / 备注 |
|------|----------|----------------|
| Rust | stable | edition 2021 |
| Slint | **1.18.1** | `backend-winit` + `renderer-software` + `software-renderer-systemfonts` + `compat-1-18` + `std`；strict 模式已验证编译通过——详见 AGENTS.md「Slint 1.18 已验证的限制」 |
| slint-build | **1.18.1** | build-dependency；`compile_with_config` + `with_bundled_translations("translations")` |
| bollard | **0.21** | Docker Engine API 纯 async 客户端；`LogOutput` 是 struct 不是 tuple——详见 [arch/docker-bollard.md](arch/docker-bollard.md) |
| tokio | **1.x** | `rt-multi-thread` + `macros` + `time` + `sync` + `rt`（default-features=false）；必须多线程 runtime 不能 current_thread；spawn 前必须 `handle().clone()`——详见 AGENTS.md「tokio 1.x 版本关键用法」 |
| futures | **0.3** | — |
| futures-util | **0.3** | 提供 `StreamExt`（`.next().await`），需显式引入 |
| 渲染 | — | 纯软件渲染器（`renderer-software`），**无 Vulkan/GL/WGPU 依赖**，deb 包不依赖 `libgl1` |
| 构建优化 | — | `opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip（release profile） |

## 2. 模块地图与文件交互

```
docker-panel/
├── Cargo.toml                依赖：slint 1.18.1 / bollard 0.21 / tokio / futures
├── build.rs                  slint_build::compile_with_config + with_bundled_translations("translations")
│
├── src/                       Rust 层（所有 ≤ 300 行）
│   ├── main.rs                组装入口：i18n → AppState → MainWindow → consumer/bootstrap 启动 → ui.run() → 优雅退出
│   ├── slint_gen.rs           include_modules!() 集中入口
│   ├── i18n.rs                locale 检测（zh*→zh_CN）+ Rust 侧 t()/trf()/trfp()（复数，对齐 @tr 的 `"sing" | "plural" % n`）+ slint_language()
│   ├── config.rs              AppConfig 持久化（directories + 手写 JSON 原子写）
│   │
│   ├── consumer.rs            consumer 线程：recv_timeout(200ms) Msg 泵 pump_and_apply + 3s 刷新 interval
│   ├── bootstrap.rs           6s stats 采样 interval + 首屏四列表加载 + inspect 首容器
│   │
│   ├── models.rs              纯数据结构：Msg / ContainerInfo / ImageInfo / ContainerDetail / NetworkInfo / VolumeInfo
│   ├── batch.rs               BatchState 多选状态（purge_dead）
│   │
│   ├── ui_state.rs            AppState（Arc<Mutex<>> 可共享状态）+ drain_messages + ensure_follow（日志流幂等建流）+ cleanup_counts + persist
│   ├── render.rs              CachedModel 差量同步（sync_model：数据 PartialEq 相同则跳过 set_vec）
│   ├── detail_fmt/            详情格式化：mod（公共 head/kv/format_bytes/network_counts）+ container / image / netvol
│   ├── ui_rows/               行构建：mod（hit_any 搜索命中）+ containers / images / netvol（网络/卷）
│   ├── ui_update.rs           derive_update(Msg → UiUpdate)
│   ├── ui_apply.rs            apply_detail / apply_lists / refresh_lists_now（搜索排序即时重建）
│   │
│   ├── callbacks.rs           回调绑定中枢 + restore_config（含启动焦点引导 boot_tick）
│   ├── callbacks_container.rs 容器启停/重启/删除/logs/inspect（AppActions global）
│   ├── callbacks_drawer.rs    抽屉 Tab 切换（切到日志 Tab 触发 ensure_follow）
│   ├── callbacks_image.rs     镜像删除 / 选中（AppActions global）
│   ├── callbacks_netvol.rs    网络/卷选中与删除（AppActions global）
│   │
│   └── docker/
│       ├── mod.rs             connect() + pub use 重导出
│       ├── containers.rs      list / start / stop / restart / remove
│       ├── images.rs          list / pull（含进度闭包）/ remove
│       ├── logs.rs            inspect_container / follow_logs（Stream → tx.send）
│       ├── stats.rs           fetch_stats + compute_cpu_delta（双采样 CPU%）
│       ├── netvol.rs          list_networks + list_volumes
│       ├── image_details.rs   inspect_image → ImageDetail
│       └── system.rs          cleanup prune
│
├── translations/
│   └── zh_CN/LC_MESSAGES/docker-panel.po   gettext 翻译（编译期嵌入）
│
└── ui/                        Slint 层（所有 ≤ 300 行）
    ├── app.slint              MainWindow：根级 global-shortcuts FocusScope（快捷键）+ 布局组装
    ├── theme.slint            浅/深双主题 22 语义色令牌 + theme_mode 三态
    ├── models.slint           ContainerRow / ImageRow / LogLineRow / NetworkRow / VolumeRow struct
    ├── top-bar.slint          TopBar：logo + 连接状态 + 搜索框 + 4 按钮
    ├── sub-bar.slint          SubBar：筛选 chips + 排序 + 4 视图 tabs
    ├── cards.slint             卡片网格容器（四种视图共用，含空状态 + 背景点击层）
    ├── card-container.slint    容器卡片（多选勾选 + 启停/重启/日志）
    ├── card-image.slint        镜像卡片（运行/删除）
    ├── card-netvol.slint       网络/卷卡片（共用布局，内建标记 + 容器数）
    ├── foot-bar.slint          底部 8 项资源概览
    ├── batch-bar.slint         批量操作浮动栏
    ├── drawer.slint            右侧抽屉：详情/日志/环境 三 Tab（四种资源共用）
    ├── dialogs.slint           PullDialog + CleanupDialog 弹窗
    ├── menu-layers.slint       浮动菜单层：排序菜单 + 卡片「···」菜单（须在卡片区之后渲染，否则被卡片 clip 裁掉）
    └── components.slint        IconBtn / Dot / CheckCell / MenuItem / Dropdown / TipLayer 等原子组件
```

### 依赖方向（严格单向，无循环）

```
main.rs
  ├── consumer.rs    → ui_state + ui_rows + ui_update + ui_apply + slint_gen
  ├── bootstrap.rs   → docker::{fetch_stats, inspect_container, list_*}
  ├── ui_state.rs    → models.rs + docker::{compute_cpu_delta, inspect_container, list_*}
  ├── ui_apply.rs    → slint_gen (MainWindow) + ui_state::DetailTuple + ui_rows
  ├── callbacks.rs   → slint_gen + ui_state::SharedState + docker + config
  ├── callbacks_container.rs / callbacks_image.rs → 同 callbacks.rs
  └── docker/        → 纯 async 层，不反依赖任何上层模块

ui/*.slint           app.slint 是唯一组装入口，import 其余子文件，
                     子文件之间不互相 import（避免 Slint 跨文件 global 兼容性坑）
```

## 3. 文件规范

所有源码文件 ≤ 300 行（当前最大 components.slint 294 / callbacks.rs 280）。

**Slint 拆分原则**详见 [arch/ui-interaction.md](arch/ui-interaction.md) §2；**Rust 拆分原则**详见 [arch/threading.md](arch/threading.md) §4。核心口径：

- app.slint 做组装层，子文件之间不互相 import
- consumer.rs 独占 Receiver，pump_and_apply 两阶段泵
- 所有 spawn 都走 state.rt（锁内 clone handle → drop → spawn）
- 所有 Docker 操作结果都走 `tx.send(Msg)`，不直接碰 Slint Model
