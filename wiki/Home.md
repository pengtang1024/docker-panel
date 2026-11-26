# Docker Panel Wiki 首页

> 一句话：给第一次接触 Docker Panel 的人看的入口页——先判断它是否适合你，再按目的进入安装、使用或排障。

**仓库**：[GitHub](https://github.com/pengtang1024/docker-panel) ｜ [Gitee](https://gitee.com/pengtang1024/docker-panel)

**Wiki**：GitHub Wiki 与 Gitee Wiki 内容一致；本仓库 `wiki/` 目录是双站 Wiki 的规范源。

> 版本说明：本页不写死“当前版本”。版本号以仓库 Cargo.toml 与 CHANGELOG.md 为准，逐版本细节见 [Changelog](Changelog)。

## 按目的选入口

| 你的目的 | 直接读 | 你会看到 |
| --- | --- | --- |
| 先把它装起来 | [Installation](Installation) | 平台范围、deb 下载与校验、源码构建、卸载与配置 |
| 已装好，想知道每个按钮 | [Usage](Usage) | 快捷键、容器台账、Compose 分组、抽屉、弹窗、自动更新 |
| 出问题了 | [FAQ](FAQ) | 连不上 Docker、中文字体、更新失败、误删等按症状排查 |
| 想看版本改了什么 | [Changelog](Changelog) | 近几个版本的精简摘要与完整 CHANGELOG 入口 |
| 想改代码 / 提 PR | [Contributing](Contributing) | 开发环境、门禁、Git Flow、提交格式与 PR 流程 |

## 1. 这是什么

Docker Panel 是一款极简 Docker Engine 桌面管理面板：给“只想看看容器状态、启停一下、翻翻日志”的人用，启动即用，不装 Electron 运行时，不要求 GPU 驱动，单个可执行文件即可运行。

| 维度 | 事实 |
| --- | --- |
| 技术形态 | Rust 单二进制 + Slint 原生 GUI + 纯软件渲染器 |
| 运行开销 | 零 GPU 依赖、零 Electron / Node 运行时；不常驻后台、不注册系统服务、不上传数据 |
| 平台 | Linux（当前唯一验证与支持的平台）；Debian / Ubuntu amd64 deb 是唯一官方发行物 |
| 连接方式 | 本机 Docker unix socket `/var/run/docker.sock`；`DOCKER_HOST` 只识别 `unix://` |
| 许可 | 自有代码 MIT；Slint 采用 Royalty-free 2.0 许可分支 |

## 2. 30 秒上手

1. 确认 Docker 已在运行、当前用户能免 sudo 访问 socket：`docker ps`。
2. 从 [GitHub Releases](https://github.com/pengtang1024/docker-panel/releases) 或 [Gitee Releases](https://gitee.com/pengtang1024/docker-panel/releases) 下载 `docker-panel_<版本>_amd64.deb` 与同目录的 `checksums.txt`，校验 `sha256sum -c checksums.txt --ignore-missing`。
3. 安装：`sudo dpkg -i docker-panel_<版本>_amd64.deb`。
4. 从应用菜单启动 **Docker Panel**，默认进入容器台账。

顶栏若显示未连接：`sudo usermod -aG docker $USER` 后重新登录。完整步骤、用户级免授权安装与源码构建见 [Installation](Installation)；交互细节见 [Usage](Usage)。

## 3. 它能做什么

| 能力 | 要点 |
| --- | --- |
| 统一列表布局 | 容器 / 镜像 / 网络 / 卷四视图全部是列表，无侧栏、无卡片网格；容器走密集台账（概览带 → 表头 → 30px 虚拟化行），其余三个是表格列表 |
| 台账内联 Compose 树 | 项目带 → 服务带 → 容器行；分组带显示组内 CPU / 内存 / 磁盘之和，收起时数字照旧；分组是分组不是筛选器 |
| 组级操作 | 分组带按钮作用于组内全部成员：启动 / 重启整组、分组合并日志、停止 / 删除全部 |
| 右侧抽屉 | 容器有详情 / 日志 / 环境三个 Tab；镜像 / 网络 / 卷为单页详情；日志窗口最多保留 2000 行 |
| 操作与即时性 | 行内启停 / 重启 / 日志 / 更多，容器多选批量；列表刷新由 Docker `/events` 事件驱动，400ms 合并窗口，无定时兜底轮询 |
| 应用内自动更新 | 静态清单三级回退，清单只广播版本号，完整性只认同 Release 的 `checksums.txt`（SHA-256）；验签与校验不通过即拒绝安装 |
| 主题与配置 | 浅色 / 深色 / 跟随系统三态主题；界面文案支持七语言（简体中文 / 英语 / 西班牙语 / 法语 / 德语 / 俄语 / 葡萄牙语），按 `LC_ALL` > `LC_MESSAGES` > `LANG` 跟随系统 locale 自动选择，缺失译文回落英文源串；界面状态持久化到 `config.json`，不支持日语 |

实现层（三线程模型、模块地图、列宽契约）见主仓 docs/architecture.md 与 docs/arch/；本 Wiki 不重复实现细节。

## 4. 动手前先看：已知边界

| 边界 | 说明 |
| --- | --- |
| 删除无二次确认 | 容器删除带 `force=true`，立即执行、不可恢复；批量删除前先确认选择范围 |
| 镜像行 ▶ 只回显提示 | 尚未接入真正的 `docker run`，不会创建并启动容器 |
| 远程 Docker 不支持 | `DOCKER_HOST` 仅识别 `unix://` 路径，不支持 tcp / ssh |
| 日志有窗口上限 | 断流补齐仅 `json-file` 驱动生效，单次最多 2000 行；日志窗口最多保留 2000 行 |
| 更新仅 Linux | 自动更新只支持 Linux 的 deb / 裸二进制；开发构建与其它平台只提示并打开下载页 |
| 单实例前置为尽力而为 | GNOME Wayland 下第二个实例不会自动置前，需从面板坞 / 任务栏切换 |

完整列表见 [FAQ](FAQ) 与仓库 README 的“已知限制”一节。

## 5. 文档地图

Wiki 六页：[Home](Home)（本页）· [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)。

主仓文档（以仓库 develop 分支为准）：README.md、CONTRIBUTING.md、SECURITY.md、SUPPORT.md、docs/configuration.md、docs/troubleshooting.md、docs/architecture.md、docs/git-flow.md、docs/roadmap.md、CHANGELOG.md。

## 页面导航

[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)
