# 配置、命令行与快捷键手册

> **一句话**：给要改界面状态、查环境变量或确认命令行与快捷键的 Linux 桌面用户——配置只读写一个 `config.json`，字段缺失或越界只回落该字段。

**对应源码**：`src/config.rs`、`src/cli.rs`、`src/i18n.rs`、`src/single_instance.rs`、`src/main.rs`、`src/models.rs`、`src/update/mod.rs`、`src/ui_update/update.rs`、`src/ui_state/config_io.rs`、`ui/app.slint`、`ui/list-area.slint`｜**相关页面**：[故障排查](troubleshooting.md)

## 0. 本页导航

**结论**：本页六节覆盖配置文件、环境变量、命令行、快捷键与更新调度，按表直达对应章节。

| # | 章节 | 你会在这里找到 |
|:--:|------|----------------|
| 1 | [配置文件](#1-配置文件) | 路径、加载与字段级回落、写入、15 个字段、重置 |
| 2 | [环境变量](#2-环境变量) | Docker socket、配置根目录、锁目录、语言检测 |
| 3 | [命令行参数](#3-命令行参数) | `--version` / `--help` 与未知参数的处理时机 |
| 4 | [全局快捷键](#4-全局快捷键) | 搜索、刷新、切视图、`Esc` 归还焦点 |
| 5 | [自动更新调度](#5-自动更新调度) | 启动检查、24h 节流、6h 轮询、重启顺序 |
| 6 | [相关文档](#6-相关文档) | 上下游页面与权威源 |

## 1. 配置文件

**结论**：应用只读写一个 `config.json`，缺字段或值越界只回落该字段；只有整份 JSON 损坏时才按默认重建。

### 路径

| 条件 | 路径 |
|------|------|
| `XDG_CONFIG_HOME` 已设置且非空 | `$XDG_CONFIG_HOME/docker-panel/config.json` |
| 缺失或为空 | `$HOME/.config/docker-panel/config.json` |

> [!IMPORTANT]
> 应用只写这一个文件：不常驻后台、不注册服务、不上传数据，也不写日志文件（`src/config.rs:194-201`）。写入失败是**静默**的——配置丢失不影响主流程。

### 加载与字段级回落

`src/config.rs:67-152` 按以下顺序处理，损坏的文件不会被整个丢弃：

| 情况 | 结果 |
|------|------|
| 文件不存在 | 使用内置默认（首次启动，不创建文件） |
| 文件存在且 JSON 合法 | 逐字段读取，每个字段独立校验 |
| 文件存在但 JSON 损坏 | 整份按内置默认填充 |
| 某字段类型不符、缺字段或值越界 | **该字段**回落默认值，其余字段照常生效 |

校验规则：`theme` `0..=2`、`view` `0..=3`、`filter` 只接受三个字符串、`sort_field` 允许 `-1..=4`、三个资源排序字段 `0..=10`、`last_update_check` 要求非负。

### 写入

`src/config.rs:154-191` 手工拼 JSON（零额外依赖），先写 `config.json.tmp` 再 `rename` 原子替换；字符串做最小转义（引号 / 反斜杠 / 换行 / 制表符 / 控制字符）。

```json
{
  "theme": 0,
  "view": 0,
  "filter": "all",
  "sort_field": 0,
  "sort_desc": true,
  "search": "",
  "log_follow": true,
  "sort_images": 0,
  "sort_networks": 0,
  "sort_volumes": 0,
  "check_updates": true,
  "auto_install": true,
  "auto_restart": true,
  "last_update_check": 0,
  "compose_collapsed": []
}
```

### 完整字段表

字段定义 `src/config.rs:11-39`，默认值 `:41-61`，全部 **15** 个字段如下（顺序与 `save()` 一致）：

| 字段 | 类型 | 取值 / 默认 | 说明 |
|------|------|-------------|------|
| `theme` | i64 | `0`=跟随系统、`1`=浅色、`2`=深色；默认 `0`；加载校验 `0..=2` | 三态主题；跟随系统靠 `gsettings`（GNOME）/ GTK `settings.ini` 检测 |
| `view` | i64 | `0`=容器、`1`=镜像、`2`=网络、`3`=卷；默认 `0`；校验 `0..=3` | 启动恢复到的视图 |
| `filter` | String | `all` / `running` / `stopped`；默认 `"all"`；只接受这三个值 | 容器视图的状态筛选 |
| `sort_field` | i64 | `0`=名称、`1`=CPU、`2`=内存、`3`=磁盘、`4`=状态；默认 `0`；允许 `-1..=4` | 容器台账排序；`-1` 表示按 Compose 分组排序 |
| `sort_desc` | bool | 默认 `true` | 容器台账排序方向（降序） |
| `search` | String | 默认 `""` | 搜索框内容，启动恢复 |
| `log_follow` | bool | 默认 `true` | 日志跟随开关 |
| `compose_collapsed` | Vec<String> | 默认 `[]` | 存的是**收起**的分组键：`project:<名>` / `svc:<项目>/<服务>`；不在集合里即为展开 |
| `sort_images` | i64 | 默认 `0`；加载校验 `0..=10` | 镜像列表排序状态 |
| `sort_networks` | i64 | 默认 `0`；加载校验 `0..=10` | 网络列表排序状态 |
| `sort_volumes` | i64 | 默认 `0`；加载校验 `0..=10` | 卷列表排序状态 |
| `check_updates` | bool | 默认 `true` | 启动时检查更新 |
| `auto_install` | bool | 默认 `true` | 检查到新版本后自动下载安装 |
| `auto_restart` | bool | 默认 `true` | 安装成功后自动重启应用 |
| `last_update_check` | i64 | unix 秒；默认 `0` | 上次检查时间，用于 24h 节流 |

三个资源排序字段共用 `ResourceSort` 编码：**低 3 位 = 列下标，第 3 位（值 `8`）= 降序标记**（`src/models.rs:302-349`，Slint 侧由 `ui/list-area.slint` 的 `SortCodec` 解包），越界回落默认；三个视图各自独立，切走再回来仍保留。

> [!NOTE]
> 口径提示：上表是配置文件字段**缺失或越界时的回落默认**（`AppConfig::default()`）。首次启动且完全没有配置文件时，界面实际默认按 Compose 分组排序、升序（`src/ui_state/config_io.rs:42-63`，对应 `sort_field=-1` / `sort_desc=false`），与表中口径不同但互不冲突。

### 重置

删除 `config.json` 即回默认；下次启动按内置默认重建界面状态，不残留旧文件。

```bash
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/docker-panel/config.json"
```

## 2. 环境变量

**结论**：只有 `DOCKER_HOST` 影响连接，且只认 `unix://`；其余变量只决定配置/锁路径与界面语言。

| 变量 | 作用 | 回落 / 边界 |
|------|------|-------------|
| `DOCKER_HOST` | 指定 Docker socket 路径 | **只识别 `unix://` 路径**，默认 `/var/run/docker.sock`；`tcp://` / `http://` / `ssh://` 不会被读取，设置后仍连本机默认 socket。见 [故障排查](troubleshooting.md) |
| `XDG_CONFIG_HOME` | 配置文件根目录 | 缺失或为空回退 `$HOME/.config`，完整路径见「路径」 |
| `XDG_RUNTIME_DIR` | 单实例锁目录 | 锁文件 `$XDG_RUNTIME_DIR/docker-panel-<user>.sock`；缺失或为空回退 `/tmp`，仍带用户名后缀避免多用户互撞 |
| `LANG` / `LC_ALL` / `LC_MESSAGES` | 界面语言检测 | `zh*` → 简体中文（zh_CN）；其它一律回落英文 msgid 源串。优先级 `LC_ALL` > `LC_MESSAGES` > `LANG`（`src/i18n.rs:84-102`） |
| `WAYLAND_DISPLAY` | 桌面坞图标匹配 | 仅当该变量存在时调用 `slint::set_xdg_app_id("docker-panel")`，让坞正确匹配应用图标（`src/main.rs:108-112`） |

## 3. 命令行参数

**结论**：`--version` / `--help` 在拿单实例锁之前处理，已有实例在跑也照常可用；其余任何参数都按正常启动 GUI 处理。

用法：`docker-panel [选项]`（`src/cli.rs`）。

| 参数 | 行为 | 时机 / 边界 |
|------|------|-------------|
| `--version` / `-V` | 打印 `docker-panel <version>` 后退出 | **在拿单实例锁之前处理**，已有实例在运行时照常可用；不初始化 Slint、不连 Docker |
| `--help` | 打印帮助（含「用法: `docker-panel [选项]`」）后退出 | 同样在拿锁之前处理 |
| 其余任何参数（含未知参数） | 一律按「正常启动 GUI」处理 | 桌面入口可能带额外参数，报错退出会破坏启动；第一个被识别的参数决定动作 |

不带参数直接启动图形界面。`--version` 可用于确认二进制版本，不会被单实例锁挡住。

## 4. 全局快捷键

**结论**：快捷键由根级 `FocusScope` 统一处理；没有焦点元素时 Slint 完全不派发按键，所以启动和弹窗关闭后都要归还焦点。

定义于 `ui/app.slint:117-165`，根级 `FocusScope` 名为 `global-shortcuts`。

| 快捷键 | 行为 |
|--------|------|
| `Ctrl+K` | 聚焦搜索框 |
| `Ctrl+R` | 手动刷新 |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` / `Ctrl+4` | 切换到 容器 / 镜像 / 网络 / 卷 视图 |
| `/` | 聚焦搜索框（已在搜索框内输入时由 `TextInput` 先消费） |
| `Esc` | 关闭排序菜单 / 卡片菜单 / 抽屉 / 拉取弹窗 / 清理弹窗 / 更新弹窗 / 设置弹窗；若有多选则清空多选；最后把焦点归还全局快捷键层 |

> 无焦点元素时 Slint 完全不派发按键，因此根级 `FocusScope` 包裹全部 UI，并做启动焦点引导（延时 300ms 投递 `boot_tick`）；弹窗/抽屉关闭导致内部焦点悬空时，`Esc` 分支会主动调 `global-shortcuts.focus()` 归还焦点。

## 5. 自动更新调度

**结论**：启动 10s 后查一次，两次启动检查间隔 24h，常驻每 6h 醒来判断窗口；自动安装后必须**先释放单实例锁、再拉起新进程**。

| 环节 | 行为 |
|------|------|
| 启动检查 | 启动 **10s** 后检查一次 |
| 节流 | 两次启动检查间隔 **24h**，`last_update_check`（unix 秒）记录上次时间 |
| 常驻轮询 | 每 **6h** 醒来判断是否满节流窗口，未到期不发请求 |
| 自动安装 | `auto_install` 开启时，检查到新版本即下载 + SHA-256 校验 + 安装 |
| 自动重启 | `auto_restart` 开启且安装成功后，**3s 倒计时**（让「正在自动重启…」文案上屏）→ 退出事件循环 → **退出流程先释放单实例锁，再由独立短命 launcher 延时 2s `exec` 新二进制** |

> 重启的顺序是硬要求：**先释放锁、后拉起新进程**。反过来（0.7.12–0.7.14 的写法）新进程启动时会连上旧 listener、判为「已在运行」后退出，旧进程随后也退出 —— 应用直接消失，用户必须手动再开一次。现在顺序由代码结构保证：`src/ui_update/update.rs:100-102` 只负责 3s 后 `quit_event_loop()`，拉起动作放在 `src/main.rs:166-186` 的退出流程里，经 `single_instance::release_then`（`src/single_instance.rs:94-97`，先 `drop(guard)` 再调用回调）执行 `update::install::restart_detached`（`src/update/install.rs:152-158`，`sh -c 'sleep N; exec <exe>'`，路径作为位置参数传入、不做字符串拼接）。launcher 只 `sleep` 一次不重试：锁被其它实例长期持有时新进程立刻退出，不会堆积进程。手动「重启应用」按钮走同一条路径。

三个开关默认开启，可在设置页（顶栏齿轮）或更新弹窗逐项关闭，关闭后持久化到 `config.json` 对应字段：`check_updates` / `auto_install` / `auto_restart`。调度常量见 `src/update/mod.rs:29-34`（`CHECK_INTERVAL_SECS` / `SCHEDULER_TICK_SECS` / `STARTUP_DELAY_SECS`）、3s 倒计时见 `src/ui_update/update.rs:11` 与 `:100-102`、launcher 延时见 `src/update/install.rs:143`。校验边界与三级回退见 [更新清单与 CDN 分发](arch/update-manifest.md)，失败处理见 [故障排查](troubleshooting.md)。

## 6. 相关文档

**结论**：排障先看第一行，设计细节再往下钻。

| 文档 | 内容 |
|------|------|
| [故障排查](troubleshooting.md) | 连接、字体、列表、日志、更新的排查路径 |
| [架构设计](architecture.md) | 技术栈与模块地图 |
| [线程模型](arch/threading.md) | 三线程、消息类型与事件订阅 |
| [更新清单与 CDN 分发](arch/update-manifest.md) | 三级回退、checksums 与 fail closed |
| [Git Flow 规范](git-flow.md) | 发版门禁与 CI 系统依赖 |
| [README](../README.md) | 功能总览 |
| [Wiki（GitHub）](https://github.com/pengtang1024/docker-panel/wiki) | 社区 Wiki（需外网） |
| [Wiki（Gitee）](https://gitee.com/pengtang1024/docker-panel/wikis) | 国内可直连的镜像站 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [README](../README.md) | [README 文档地图](../README.md#文档地图) | [故障排查](troubleshooting.md) |
