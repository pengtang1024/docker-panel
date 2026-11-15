# 参与贡献

docker-panel 是 Rust 单二进制 + Slint 原生 GUI 的 Docker Engine 桌面管理面板，**面向 Linux 桌面**，纯软件渲染（零 GPU / Electron / Node 运行时）。

欢迎缺陷报告、功能建议与代码贡献。开始之前请先读这两份：

- [AGENTS.md](AGENTS.md) — 项目速查：技术栈精确版本、线程模型、消息类型、36 条编码规范、Slint 1.18 已验证限制
- [docs/agents-rules.md](docs/agents-rules.md) — 规则配置层（规则唯一权威源）：红线约束、四阶段门禁、文档同步规范

> AGENTS.md 最初是给 AI 编码助手写的，但它是本项目最准确的实现地图。人类贡献者同样应该先读它，尤其是「Slint 1.18 已验证的限制」与「代码规范」两节——踩过的坑都在那里。

## 开发环境

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

`libfontconfig1-dev` 是必需的：`yeslogic-fontconfig-sys` 的 build script 依赖 `fontconfig.pc`，缺它会直接报
`The system library 'fontconfig' required by crate 'yeslogic-fontconfig-sys' was not found`。

### Docker 权限

当前用户必须能免 sudo 访问 socket，否则应用启动后顶栏显示 **○ 未连接**：

```bash
sudo usermod -aG docker $USER
# 重新登录（或 newgrp docker）后验证
docker ps
```

应用只识别 `unix://` 形式的 `DOCKER_HOST`（可指向其它本机 socket），**不支持 tcp / ssh 远程地址**。

### 克隆与运行

```bash
git clone https://github.com/pengtang1024/docker-panel.git   # Gitee 镜像：https://gitee.com/pengtang1024/docker-panel.git
cd docker-panel
cargo run
```

`cargo run` 是 debug 构建，启动慢、体积大但编译快，适合迭代。发布形态用 `cargo build --release`。

## 构建与验证

| 命令 | 用途 |
|------|------|
| `cargo run` | 开发调试 |
| `cargo build --release` | 发布构建（`opt-level="z"` + LTO + `codegen-units=1` + `panic="abort"` + strip，二进制约 13MB） |
| `cargo fmt --all -- --check` | 格式检查（CI 门禁 1） |
| `cargo clippy --all-targets --all-features -- -D warnings` | 静态检查，警告即失败（CI 门禁 2） |
| `cargo test` | 单元测试（CI 门禁 3） |
| `cargo check --all-targets` | 快速类型检查 |

**提交前四项必须全绿**：`fmt` / `clippy -D warnings` / `test` / `build --release`。CI 的 `quality.yml` 用固定工具链 `1.99.1` 跑前三项，`license.yml` 用 `cargo-deny` 按 [deny.toml](deny.toml) 校验许可证——引入 GPL / AGPL / LGPL / SSPL / BUSL / Commons Clause 会让流水线失败。

> 提交前不要忘了 `grep -ri d[v]kit .` 必须零残留（见下方「自检清单」；写成字符类是为了让本文件自身不命中该词）。

## 分支与提交

本项目严格走 Git Flow，完整规则见 [docs/git-flow.md](docs/git-flow.md)。

| 分支 | 职责 |
|------|------|
| `master` | 已发布版本，只接受 `release finish` / `hotfix finish` 产物 |
| `develop` | 集成分支，**日常 PR 的目标分支** |
| `feature/*` | 新功能，从 `develop` 切出，合回 `develop` |
| `bugfix/*` | 缺陷修复，从 `develop` 切出，合回 `develop` |
| `release/*` / `hotfix/*` | 发版与线上紧急修复，属维护者职责 |

**PR 目标分支一律 `develop`。** 不要向 `master` 直接提 PR——`master` 的每次提交都必须对应一个纯数字 tag `X.Y.Z`（不带 `v` 前缀），由发版流程创建。

提交信息格式固定为 `type(scope): 中文描述`：

```
feat(update): 发布后自动提交清单到 master，jsDelivr 备源生效
fix(ui): 开抽屉时保留磁盘列（阈值 1000→860）
docs: 同步事件驱动刷新表述与 Wiki 双站点，README 平台范围收敛为 Linux
```

`type` 取 `feat` / `fix` / `perf` / `refactor` / `chore` / `docs` / `ci`；`scope` 是受影响的模块（`update` / `ui` / `rows` / `release` / `i18n` 等），可省略。

## 第一次贡献：完整走一遍

以「修一处文档错字」为例，从零到发起 PR 的完整命令序列（分支名换成你自己的）：

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
| 忘了同步文档 | 照「文档同步义务」那张表过一遍；改了源码结构却不动 README 结构树，会被要求补 |
| 分支落后于 `develop` | 推送前 `git fetch upstream`，再 `git merge upstream/develop`（或在你**尚未**推送的个人分支上 `git rebase upstream/develop`） |

Gitee 侧与 GitHub 的唯一差异是 fork 与 PR 都在 gitee.com 上完成，命令完全相同（把 clone 地址换成 `https://gitee.com/<你的用户名>/docker-panel.git`）。两站**不互相同步 PR**，选一站提一次即可，不需要两边都提。

## PR 前检查清单

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 通过
- [ ] `cargo test` 通过
- [ ] `cargo build --release` 通过
- [ ] `grep -ri d[v]kit .` 零残留（该旧名已废弃，任何文件、注释、字符串、包名、二进制名、desktop 入口都禁止出现）
- [ ] 改动的源码/文档文件，去除头部与注释后正文 **≤ 300 行**（超过必须按职责拆分）
- [ ] **涉及 UI 的改动附截图**（见下节）
- [ ] 按「文档同步义务」更新受影响的文档
- [ ] Wiki 变更同时推 GitHub 与 Gitee 两站

## UI 改动的验收方式

本项目运行在 Wayland 下，`scrot` / `gnome-screenshot` / `maim` 全部黑屏。**UI 验收统一走 `agkit shot`**（按 xdg app_id 定位窗口，零弹窗）：

```bash
cargo build --release
./target/release/docker-panel &
agkit shot --app docker-panel -o /tmp/dp.png    # 按 xdg app_id 定位并截图
agkit shot --list-apps                          # 查看已记录的区域
```

`--app` 定位不到时才退回 `--region WxH+X+Y`（坐标基准是**整个虚拟桌面**，双屏时注意偏移）。

**修复 UI 问题必须附截图证据**，不要只靠口述描述。任何涉及界面的 PR，描述里请贴 `agkit shot` 的截图；纯逻辑改动不需要。

Slint MCP 已降级为**调试数据暴露**通道：只在需要读元素树 / 属性值（如核对 `absolutePosition.x` 与 `size.width`）时临时 `cargo build --release --features slint/mcp`，它不承担截图与验收。

## 代码规范速览

完整 36 条见 [AGENTS.md](AGENTS.md)「代码规范」，以下是新贡献者最容易踩的 10 条：

1. **`mpsc::Receiver<Msg>` 由 consumer 线程独占，绝不放进 Mutex。** 阻塞 `recv_timeout()` 时不能持有 state 锁，否则主线程与 UI 回调全卡死。
2. **所有 spawn 走 `state.rt`**：`let (rt, tx) = { let Ok(s) = state.lock() else { return }; (s.rt.handle().clone(), s.tx.clone()) }; drop(s); rt.spawn(...)`。
3. **所有 Docker 结果走 `tx.send(Msg)`**，不直接碰 Slint Model；UI 更新统一由 consumer 泵完成。
4. **两阶段铁律**：锁内只做分片 drain 与 build，drop 锁之后才 `invoke_from_event_loop`，闭包内绝不触碰 state。
5. **锁中毒禁止静默吞掉**：用 `crate::lock::poison("上下文")` / `crate::lock::acquire(&state, "上下文")` 留痕。
6. **Slint 子文件不互相 import**：delegate 只声明 `callback`，由 `app.slint` 注入实现，避免跨文件 global 兼容坑。
7. **列表即时性靠 Docker `/events` 事件订阅**，禁止再加定时兜底轮询（会把真实 bug 藏成「最多慢 N 秒」）。
8. **UI 状态单一写入方**：排序 / 筛选 / 视图等用户操作属性由 UI 回调独占写入，consumer 只刷数据、不回写。
9. **超高频 Msg 不得逐条 invoke**：统一走 consumer 分批 drain + 120ms 节流（纯日志批 250ms）。
10. **错误处理**：Docker API 错误统一转 `Result<T, String>` 上抛，UI 层展示中文文本；不吞异常。

另外几条红线与流程要求（[docs/agents-rules.md](docs/agents-rules.md)）：

- **R3 架构设计优先于代码实现**——实现与架构冲突时改实现，不改架构，并记录冲突。
- **R4 复用率 ≥ 80%**——新增代码前先找现成实现，不足时触发二次校验。
- **四阶段门禁**：S1 需求解析 → S2 方案生成 → S3 代码输出 → S4 最终校验，任一阶段不达标不得进入下一阶段。小改动可简化，但「先想清楚再写」是硬要求。

## 文档同步义务

源码与功能改动必须同步文档（完整触发表见 [docs/agents-rules.md](docs/agents-rules.md) §6）。常用映射：

| 改了什么 | 要同步哪里 |
|----------|-----------|
| 源码文件增删改名 | README 项目结构树 · AGENTS.md 模块地图 · docs/architecture.md |
| Slint 文件增删 | 上述三项 + docs/arch/ui-interaction.md |
| 依赖版本升级 | README 依赖表 · AGENTS.md 技术栈表 · docs/architecture.md · Wiki Architecture |
| 架构变更 | AGENTS.md 线程模型与消息表 · docs/architecture.md 与 docs/arch/ · README 架构 ASCII 图 · Wiki Architecture |
| 用户可见功能变更 | CHANGELOG.md · README 功能特性 · AGENTS.md 已知限制 · Wiki Home 与 Usage |
| 规则变更 | docs/agents-rules.md · AGENTS.md 规则地图 |
| 发版 | CHANGELOG 新增版本段落 · 三处版本号同步（`Cargo.toml` / `packaging/debian/control` / `CHANGELOG.md`）· Wiki Home 版本声明 |

### 双份 Wiki 同步

Wiki 有**两个独立 git 仓库**：

| 站点 | 仓库 |
|------|------|
| GitHub | `https://github.com/pengtang1024/docker-panel.wiki.git` |
| Gitee | `https://gitee.com/pengtang1024/docker-panel.wiki.git` |

任何 Wiki 变更必须**同一批改动同时推两站**，禁止只更新一侧。页面为 Home / Install / Usage / Architecture / FAQ / _Sidebar。

两条硬规则：

- **内链用标准 Markdown 相对链接**（如 `[安装与构建](Install)`，指向 Wiki 页面名）。`[[Page|Label]]` 是 GitHub Gollum 的 MediaWiki 语法，Gitee 的 CommonMark 会把它原样输出成纯文本（坏链）。
- **图片走 jsDelivr CDN**：`https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@develop/assets/screenshots/<name>.png`。Wiki 仓库不放二进制图片。

## 翻译贡献

界面文案用 gettext，当前只有简体中文一门（英文是源码里的 msgid 源串）。新增语言、修翻译、占位符与复数规则见 **[docs/translations.md](docs/translations.md)**。

## 求助与沟通

- 缺陷与功能建议：[GitHub Issues](https://github.com/pengtang1024/docker-panel/issues) · [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues)
- 提问前请先看 [SUPPORT.md](SUPPORT.md)（该附哪些信息）与 [docs/troubleshooting.md](docs/troubleshooting.md)（按症状排障）
- **安全漏洞走私有渠道**，不要发公开 Issue，见 [SECURITY.md](SECURITY.md)
- 参与本项目即表示同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)（Contributor Covenant 2.1）
- 本项目没有邮件列表 / 论坛 / 商业支持频道，所有沟通走上面两个 Issue 入口

## 相关文档

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
