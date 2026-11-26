# 参与贡献

> **一句话**：给想提缺陷、想提功能、想交代码的贡献者看的一页——环境怎么装、门禁怎么过、PR 怎么提，照着做就不会返工。

**对应源码**：[Cargo.toml](Cargo.toml)（MSRV 与 release profile）、[deny.toml](deny.toml)（许可证 allow-list）、[.github/workflows/](.github/workflows/)（CI 门禁）｜**规则权威源**：[docs/agents-rules.md](docs/agents-rules.md)

> [!IMPORTANT]
> **先读 [AGENTS.md](AGENTS.md)，再动手。** 它最初是给 AI 编码助手写的，但至今是本项目最准确的实现地图：技术栈精确版本、线程模型、消息类型、36 条编码规范、Slint 1.18 已验证的限制都在里面。人类贡献者尤其要看「Slint 1.18 已验证的限制」与「代码规范」两节——踩过的坑都记在那里。

## 目录

**结论：12 节按「先读懂 → 再跑通 → 最后提 PR」的顺序排列——第一次贡献顺着读，查命令直接跳第 3 节与第 6 节。**

| # | 章节 | 你会在这里找到 |
|:--:|------|----------------|
| 1 | [先读两份文档](#1-先读两份文档) | 必读的文档清单与各自的用途 |
| 2 | [开发环境](#2-开发环境) | Rust / Docker / 系统库三项要求与 Docker 权限 |
| 3 | [构建与验证](#3-构建与验证) | 六条常用命令、四项合入门禁、许可证检查 |
| 4 | [分支与提交](#4-分支与提交) | Git Flow 分支职责与 `type(scope): 描述` 格式 |
| 5 | [第一次贡献全流程](#5-第一次贡献全流程) | 从 fork 到发 PR 的 7 步命令序列 + 五个常见卡点 |
| 6 | [PR 前检查清单](#6-pr-前检查清单) | 九项逐条勾选的合入前自检 |
| 7 | [UI 改动的验收方式](#7-ui-改动的验收方式) | `agkit shot` 截图验收的完整命令 |
| 8 | [代码规范速览](#8-代码规范速览) | 新贡献者最容易踩的 10 条 + 三条红线 |
| 9 | [文档同步义务](#9-文档同步义务) | 改了什么要同步哪里、Wiki 双站同步 |
| 10 | [翻译贡献](#10-翻译贡献) | gettext 文案与新增语言的入口 |
| 11 | [求助与沟通](#11-求助与沟通) | 缺陷 / 建议 / 安全问题的分流渠道 |
| 12 | [相关文档](#12-相关文档) | 本页之外该读的 11 篇 |

## 1. 先读两份文档

**结论：动手前必须读完下面两份，只读一份会在评审阶段被要求返工。**

| 文档 | 为什么必须先读 |
|------|----------------|
| [AGENTS.md](AGENTS.md) | 项目速查：技术栈精确版本、线程模型、消息类型、36 条编码规范、Slint 1.18 已验证限制 |
| [docs/agents-rules.md](docs/agents-rules.md) | 规则配置层（规则唯一权威源）：红线约束、四阶段门禁、文档同步规范 |

欢迎缺陷报告、功能建议与代码贡献。本项目是 Rust 单二进制 + Slint 原生 GUI 的 Docker Engine 桌面管理面板，**面向 Linux 桌面**，纯软件渲染（零 GPU / Electron / Node 运行时）。

## 2. 开发环境

**结论：Linux 桌面上装齐「Rust stable + 本机 Docker + 三个系统库」，`git clone` 后即可 `cargo run`，无需额外配置。**

| 要求 | 说明 |
|------|------|
| Rust | stable，MSRV **1.99**；`rust-toolchain.toml` 已固定 stable 通道并带 `rustfmt` / `clippy`，直接 `git clone` 后无需额外配置 |
| Docker | 本机已运行 Docker Engine，默认 unix socket `/var/run/docker.sock` |
| 系统库 | 见下方 `apt` 命令（fontconfig / xkbcommon / wayland） |

```bash
# Debian / Ubuntu 系统依赖
sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev

# 其它发行版的等价包名
# Fedora/RHEL: dnf install gcc make pkgconf-pkg-config fontconfig-devel libxkbcommon-devel wayland-devel
# Arch:        pacman -S base-devel pkgconf fontconfig libxkbcommon wayland
```

> [!WARNING]
> **`libfontconfig1-dev` 缺失会直接编译失败**，不是警告。`yeslogic-fontconfig-sys` 的 build script 依赖 `fontconfig.pc`，缺它会在构建时报 `The system library 'fontconfig' required by crate 'yeslogic-fontconfig-sys' was not found`。

### Docker 权限

**结论：当前用户必须能免 sudo 访问 socket，否则应用启动后顶栏显示 ○ 未连接。**

```bash
sudo usermod -aG docker $USER
# 重新登录（或 newgrp docker）后验证
docker ps
```

应用只识别 `unix://` 形式的 `DOCKER_HOST`（可指向其它本机 socket），**不支持 tcp / ssh 远程地址**。

### 克隆与运行

**结论：开发用 `cargo run`（debug 构建），发布形态用 `cargo build --release`。**

```bash
git clone https://github.com/pengtang1024/docker-panel.git   # Gitee 站点：https://gitee.com/pengtang1024/docker-panel.git
cd docker-panel
cargo run
```

`cargo run` 是 debug 构建，启动慢、体积大但编译快，适合迭代。

## 3. 构建与验证

**结论：提交前 `fmt` / `clippy -D warnings` / `test` / `build --release` 四项必须全绿，缺一项不予合入。**

| 命令 | 用途 |
|------|------|
| `cargo run` | 开发调试 |
| `cargo build --release` | 发布构建（`opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip，二进制约 13MB） |
| `cargo fmt --all -- --check` | 格式检查（CI 门禁 1） |
| `cargo clippy --all-targets --all-features -- -D warnings` | 静态检查，警告即失败（CI 门禁 2） |
| `cargo test` | 单元测试（CI 门禁 3） |
| `cargo check --all-targets` | 快速类型检查 |

CI 的 `quality.yml` 用固定工具链 `1.99.1` 跑前三项；`license.yml` 用 `cargo-deny` 按 [deny.toml](deny.toml) 校验许可证——引入 GPL / AGPL / LGPL / SSPL / BUSL / Commons Clause 会让流水线失败。

提交前不要忘了 `grep -ri d[v]kit .` 必须零残留（见第 6 节检查清单；写成字符类是为了让本文件自身不命中该词）。

## 4. 分支与提交

**结论：PR 目标分支一律 `develop`，提交信息一律 `type(scope): 中文描述`。**

| 分支 | 职责 |
|------|------|
| `master` | 已发布版本，只接受 `release finish` / `hotfix finish` 产物 |
| `develop` | 集成分支，**日常 PR 的目标分支** |
| `feature/*` | 新功能，从 `develop` 切出，合回 `develop` |
| `bugfix/*` | 缺陷修复，从 `develop` 切出，合回 `develop` |
| `release/*` / `hotfix/*` | 发版与线上紧急修复，属维护者职责 |

不要向 `master` 直接提 PR——`master` 的每次提交都必须对应一个纯数字 tag `X.Y.Z`（不带 `v` 前缀），由发版流程创建。新 tag 一律用 `git tag -s X.Y.Z -m "release: vX.Y.Z"` 签名（维护者职责），并 `git tag -v X.Y.Z` 验到 **Good signature** 才算有效；GitHub 会显示 "This tag was signed with the committer's verified signature"。**历史 tag 一律不改写、不删除、不重签、不强推覆盖**，签名约定只约束今后新建的 tag。完整规则与本地配置（SSH 签名密钥 / allowedSignersFile）见 [docs/git-flow.md](docs/git-flow.md)「签名 tag 约定」。

```
feat(update): 发布后自动提交清单到 master，jsDelivr 备源生效
fix(ui): 开抽屉时保留磁盘列（阈值 1000→860）
docs: 同步事件驱动刷新表述与 Wiki 双站点，README 平台范围收敛为 Linux
```

`type` 取 `feat` / `fix` / `perf` / `refactor` / `chore` / `docs` / `ci`；`scope` 是受影响的模块（`update` / `ui` / `rows` / `release` / `i18n` 等），可省略。

## 5. 第一次贡献全流程

**结论：从 fork 到发 PR 共 7 步，第 5 步四项门禁不过就不要提交——修到全绿比事后返工快。**

以「修一处文档错字」为例，分支名换成你自己的：

```bash
# 1. 先在托管站 fork（GitHub 或 Gitee 任选一站，两站代码等价）
git clone https://github.com/<你的用户名>/docker-panel.git
cd docker-panel

# 2. 把官方仓库加为 upstream，之后随时同步
git remote add upstream https://github.com/pengtang1024/docker-panel.git

# 3. 从最新 develop 切分支（不要从 master 切）
git fetch upstream
git switch -c fix/my-first-patch upstream/develop

# 4. 改代码 / 改文档

# 5. 本地过门禁，四项全绿再提交
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release

# 6. 提交（精确指定文件，不要 git add .）
git add README.md docs/configuration.md
git commit -m "docs: 修正自动重启机制描述与行号引用"

# 7. 推送并发起 PR，目标分支选 develop
git push -u origin fix/my-first-patch
```

发 PR 时请勾完模板里的检查清单，描述里写清**改了什么 / 为什么要改 / 怎么验证**。CI 会自动跑 `quality.yml`（fmt + clippy + test）与 `license.yml`（`cargo-deny` 校验许可证）。**涉及 UI 的改动必须附 `agkit shot` 截图**，否则无法验收。

新手最常卡住的五处：

| 卡点 | 说明 |
|------|------|
| PR 目标分支选错 | 一律选 `develop`。向 `master` 的 PR 会被拒——`master` 上每次提交都对应一个发布 tag |
| 从 `master` 切分支 | 会把发布提交的历史带进功能分支，合回 `develop` 时产生无谓的 merge 提交。一律从 `upstream/develop` 切 |
| 提交信息写成「修改文档」 | CI 不校验格式，但评审核对时必须符合 `type(scope): 中文描述` |
| 忘了同步文档 | 照第 9 节「文档同步义务」那张表过一遍；改了源码结构却不动 README 结构树，会被要求补 |
| 分支落后于 `develop` | 推送前 `git fetch upstream`，再 `git merge upstream/develop`（或在你**尚未**推送的个人分支上 `git rebase upstream/develop`） |

Gitee 侧与 GitHub 的唯一差异是 fork 与 PR 都在 gitee.com 上完成，命令完全相同（把 clone 地址换成 `https://gitee.com/<你的用户名>/docker-panel.git`）。两站**不互相同步 PR**，选一站提一次即可，不需要两边都提。

## 6. PR 前检查清单

**结论：下列九项全部勾完再点「创建 PR」，任何一项没勾都会被要求补。**

<details>
<summary><b>PR 前检查清单（9 项）</b></summary>

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 通过
- [ ] `cargo test` 通过
- [ ] `cargo build --release` 通过
- [ ] `grep -ri d[v]kit .` 零残留（该旧名已废弃，任何文件、注释、字符串、包名、二进制名、desktop 入口都禁止出现）
- [ ] 改动的源码/文档文件，去除头部与注释后正文 **≤ 300 行**（超过必须按职责拆分）
- [ ] **涉及 UI 的改动附截图**（见第 7 节）
- [ ] 按第 9 节「文档同步义务」更新受影响的文档
- [ ] Wiki 变更同时推 GitHub 与 Gitee 两站

</details>

## 7. UI 改动的验收方式

**结论：UI 改动必须附 `agkit shot` 截图证据，口述描述不算验收。**

本项目运行在 Wayland 下，`scrot` / `gnome-screenshot` / `maim` 全部黑屏。UI 验收统一走 `agkit shot`（按 xdg app_id 定位窗口，零弹窗）：

```bash
cargo build --release
./target/release/docker-panel &
agkit shot --app docker-panel -o /tmp/dp.png    # 按 xdg app_id 定位并截图
agkit shot --list-apps                          # 查看已记录的区域
```

> [!TIP]
> 先跑 `agkit shot --list-apps` 确认窗口已被识别，再截图，能少走一轮「截出黑图」的返工。`--app` 定位不到时才退回 `--region WxH+X+Y`（坐标基准是**整个虚拟桌面**，双屏时注意偏移）。

Slint MCP 已降级为**调试数据暴露**通道：只在需要读元素树 / 属性值（如核对 `absolutePosition.x` 与 `size.width`）时临时 `cargo build --release --features slint/mcp`，它不承担截图与验收。

## 8. 代码规范速览

**结论：完整 36 条在 [AGENTS.md](AGENTS.md)「代码规范」，新贡献者先守住下面 10 条就不会写出需要重写的代码。**

<details>
<summary><b>新贡献者最容易踩的 10 条规范</b></summary>

| # | 规范 | 要点 |
|:--:|------|------|
| 1 | Receiver 独占 | `mpsc::Receiver<Msg>` 由 consumer 线程独占，绝不放进 Mutex。阻塞 `recv_timeout()` 时不能持有 state 锁，否则主线程与 UI 回调全卡死 |
| 2 | 所有 spawn 走 `state.rt` | `let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(...)` |
| 3 | 结果走通道 | 所有 Docker 结果走 `tx.send(Msg)`，不直接碰 Slint Model；UI 更新统一由 consumer 泵完成 |
| 4 | 两阶段铁律 | 锁内只做分片 drain 与 build，drop 锁之后才 `invoke_from_event_loop`，闭包内绝不触碰 state |
| 5 | 锁中毒留痕 | 禁止静默吞掉：用 `crate::lock::poison("上下文")` / `crate::lock::acquire(&state, "上下文")` 留痕 |
| 6 | Slint 文件不互 import | delegate 只声明 `callback`，由 `app.slint` 注入实现，避免跨文件 global 兼容坑 |
| 7 | 即时性靠事件 | 列表即时性靠 Docker `/events` 事件订阅，禁止再加定时兜底轮询（会把真实 bug 藏成「最多慢 N 秒」） |
| 8 | 单一写入方 | 排序 / 筛选 / 视图等用户操作属性由 UI 回调独占写入，consumer 只刷数据、不回写 |
| 9 | 高频 Msg 不逐条 invoke | 统一走 consumer 分批 drain + 120ms 节流（纯日志批 250ms） |
| 10 | 错误处理 | Docker API 错误统一转 `Result<T, String>` 上抛，UI 层展示中文文本；不吞异常 |

</details>

另外三条红线与流程要求（见 [docs/agents-rules.md](docs/agents-rules.md)）：

| 规则 | 要求 |
|------|------|
| R3 架构优先 | 架构设计优先于代码实现——实现与架构冲突时改实现，不改架构，并记录冲突 |
| R4 复用率 | 复用率 ≥ 80%——新增代码前先找现成实现，不足时触发二次校验 |
| 四阶段门禁 | S1 需求解析 → S2 方案生成 → S3 代码输出 → S4 最终校验，任一阶段不达标不得进入下一阶段。小改动可简化，但「先想清楚再写」是硬要求 |

## 9. 文档同步义务

**结论：改了源码或功能就必须同步文档，按下面这张映射表查接收方，漏一处评审会打回。**

| 改了什么 | 要同步哪里 |
|----------|-----------|
| 源码文件增删改名 | README 项目结构树 · AGENTS.md 模块地图 · docs/architecture.md |
| Slint 文件增删 | 上述三项 + docs/arch/ui-interaction.md |
| 依赖版本升级 | README 依赖表 · AGENTS.md 技术栈表 · docs/architecture.md · Wiki Architecture |
| 架构变更 | AGENTS.md 线程模型与消息表 · docs/architecture.md 与 docs/arch/ · README 架构 ASCII 图 · Wiki Architecture |
| 用户可见功能变更 | CHANGELOG.md · README 功能特性 · AGENTS.md 已知限制 · Wiki Home 与 Usage |
| 规则变更 | docs/agents-rules.md · AGENTS.md 规则地图 |
| 发版 | CHANGELOG 新增版本段落 · 四处版本号同步（`Cargo.toml` / `packaging/debian/control` / `Cargo.lock` / `CHANGELOG.md`）· Wiki Home 版本声明 |

完整触发表见 [docs/agents-rules.md](docs/agents-rules.md) §6。

### 双份 Wiki 同步

**结论：Wiki 是两个独立 git 仓库，任何变更必须同一批改动同时推两站，禁止只更新一侧。**

| 站点 | 仓库 |
|------|------|
| GitHub | `https://github.com/pengtang1024/docker-panel.wiki.git` |
| Gitee | `https://gitee.com/pengtang1024/docker-panel.wiki.git` |

页面为 Home / Install / Usage / Architecture / FAQ / _Sidebar。两条硬规则：

- **内链用标准 Markdown 相对链接**（标签 + 页面名，如「安装与构建」指向 `Install`）。GitHub Gollum 的 MediaWiki 双链语法 Gitee 的 CommonMark 不支持，会把它原样输出成纯文本（坏链）。
- **图片走 jsDelivr CDN**：`https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@develop/assets/screenshots/<name>.png`。Wiki 仓库不放二进制图片。

## 10. 翻译贡献

**结论：界面文案用 gettext，当前只有简体中文一门（英文是源码里的 msgid 源串），新增语言按 [docs/translations.md](docs/translations.md) 走。**

新增语言、修翻译、文案变量插值与复数规则都在那份文档里，本页不重复。

## 11. 求助与沟通

**结论：缺陷与建议只走两个 Issue 入口，安全漏洞走私用渠道，本项目没有邮件列表与商业支持。**

| 事项 | 去哪里 |
|------|--------|
| 缺陷与功能建议 | [GitHub Issues](https://github.com/pengtang1024/docker-panel/issues) · [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues) |
| 提问前该附什么信息 | [SUPPORT.md](SUPPORT.md) |
| 按症状排障 | [docs/troubleshooting.md](docs/troubleshooting.md) |
| 安全漏洞 | [SECURITY.md](SECURITY.md)（**不要发公开 Issue**） |
| 行为准则问题 | [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)（Contributor Covenant 2.1） |

参与本项目即表示同意遵守行为准则。所有沟通走上面两个 Issue 入口，没有邮件列表 / 论坛 / 商业支持频道。

## 12. 相关文档

**结论：本页覆盖流程与门禁，实现细节看下表左侧前四篇，法务与变更看最后两篇。**

<details>
<summary><b>相关文档（11 篇）</b></summary>

| 文档 | 内容 |
|------|------|
| [README.md](README.md) | 功能特性、安装、项目结构树、依赖表 |
| [AGENTS.md](AGENTS.md) | 实现地图与 36 条编码规范 |
| [docs/architecture.md](docs/architecture.md) | 架构总纲 + arch/ 主题文档 |
| [docs/git-flow.md](docs/git-flow.md) | 分支模型与发版流程 |
| [docs/configuration.md](docs/configuration.md) | config.json 字段全表 |
| [docs/troubleshooting.md](docs/troubleshooting.md) | 故障排查 |
| [docs/translations.md](docs/translations.md) | 翻译流程 |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | 行为准则与执行分级 |
| [SUPPORT.md](SUPPORT.md) | 支持渠道与提问必附信息 |
| [SECURITY.md](SECURITY.md) | 安全策略与漏洞报告渠道 |
| [CHANGELOG.md](CHANGELOG.md) | 版本变更记录 |

</details>

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [安全策略](SECURITY.md) | [README 文档地图](README.md#文档地图) | [行为准则](CODE_OF_CONDUCT.md) |
