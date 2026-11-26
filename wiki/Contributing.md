# 参与贡献

> 一句话：给想提缺陷、提功能、交代码的人看的一页——环境怎么装、门禁怎么过、PR 怎么提，照着做就不会返工。

**权威源**：仓库 CONTRIBUTING.md（完整流程）· docs/agents-rules.md（规则配置层）· docs/git-flow.md（分支与发版）· AGENTS.md（实现地图）

**相关页面**：[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog)

## 1. 先读两份文档

动手前读完仓库 AGENTS.md 与 docs/agents-rules.md：技术栈精确版本、线程模型、消息类型、编码规范、Slint 1.18 已验证限制都记在那里；只读一份会在评审阶段被要求返工。

## 2. 开发环境

Linux 桌面上装齐 Rust stable（MSRV 1.99，`rust-toolchain.toml` 已固定通道）+ 本机 Docker + 三个系统库，即可 `cargo run`：

```bash
sudo apt install build-essential pkg-config \
  libfontconfig1-dev libxkbcommon-dev libwayland-dev
sudo usermod -aG docker $USER   # 重新登录后验证 docker ps
```

缺 `libfontconfig1-dev` 会直接编译失败。应用只识别 `unix://` 形式的 `DOCKER_HOST`。

## 3. 构建与验证

提交前四项全绿，缺一项不予合入：

| 命令 | 用途 |
| --- | --- |
| `cargo fmt --all -- --check` | 格式检查 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 静态检查，警告即失败 |
| `cargo test` | 单元测试 |
| `cargo build --release` | 发布构建 |

Cargo.toml 声明 `rust-version = "1.99"`，CI 的 quality / release 流水线固定工具链 `1.99.1`；license 流水线用 cargo-deny 校验许可证。提交前还要 `grep -ri d[v]kit .` 零残留（废弃旧名禁止出现在任何文件 / 注释 / 字符串 / 二进制名中）。

## 4. 分支与提交

| 分支 | 职责 |
| --- | --- |
| `master` | 已发布版本，只接受 release / hotfix finish 产物 |
| `develop` | 集成分支，**所有 PR 的目标分支** |
| `feature/*` / `bugfix/*` | 从 develop 切出，合回 develop |
| `release/*` / `hotfix/*` | 发版与线上紧急修复，属维护者职责 |

提交信息格式：`type(scope): 中文描述`，type 取 `feat` / `fix` / `perf` / `refactor` / `chore` / `docs` / `ci`，scope 为受影响模块可省略，例如：

```text
feat(update): 发布后自动提交清单到 master，jsDelivr 备源生效
fix(ui): 开抽屉时保留磁盘列（阈值 1000→860）
docs: 同步事件驱动刷新表述与 Wiki 双站点
```

`master` 的每次提交都必须对应一个纯数字 tag `X.Y.Z`（不带 `v` 前缀），由发版流程创建，不要直接向 master 提 PR。

## 5. 第一次贡献全流程

```bash
git clone https://github.com/<你的用户名>/docker-panel.git
cd docker-panel
git remote add upstream https://github.com/pengtang1024/docker-panel.git
git fetch upstream
git switch -c fix/my-first-patch upstream/develop
# 改代码 / 改文档
cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings \
  && cargo test && cargo build --release
git add README.md docs/configuration.md    # 精确指定文件，不要 git add .
git commit -m "docs: 修正文档描述"
git push -u origin fix/my-first-patch
```

然后开 PR，目标分支选 `develop`，描述写清改了什么 / 为什么 / 怎么验证。Gitee 侧流程相同（clone 地址换成 gitee.com），两站 PR 不互相同步，选一站提一次即可。

常见卡点：PR 目标选成 master、从 master 切分支、提交信息不合格、忘记同步受影响文档、分支落后于 develop。

## 6. PR 前检查清单

- [ ] fmt / clippy -D warnings / test / build --release 四项全绿
- [ ] `grep -ri d[v]kit .` 零残留
- [ ] 改动文件去除头部与注释后正文 ≤ 300 行
- [ ] 涉及 UI 的改动附 `agkit shot` 截图证据（Wayland 下其它截图工具黑屏）
- [ ] 按文档同步义务更新受影响文档
- [ ] Wiki 变更同时同步 GitHub 与 Gitee 两站（内容一致）

## 7. UI 改动的验收方式

```bash
cargo build --release
./target/release/docker-panel &
agkit shot --app docker-panel -o /tmp/dp.png
```

先 `agkit shot --list-apps` 确认窗口被识别，定位不到时才退回 `--region`。Slint MCP 只用于调试读元素树，不承担验收。

## 8. 代码规范速览

| # | 规范 | 要点 |
| --- | --- | --- |
| 1 | Receiver 独占 | `mpsc::Receiver<Msg>` 不进 Mutex，避免阻塞 UI 拿锁 |
| 2 | spawn 走 state.rt | 锁内 clone handle → drop → spawn |
| 3 | 结果走通道 | Docker 结果统一 `tx.send(Msg)`，不直接碰 Slint Model |
| 4 | 两阶段铁律 | 锁内 drain / build，锁外 `invoke_from_event_loop`，闭包不碰 state |
| 5 | 锁中毒留痕 | 禁止静默吞 `PoisonError` |
| 6 | Slint 文件不互 import | delegate 只声明 callback，由组装层注入 |
| 7 | 即时性靠事件 | 列表刷新靠 `/events`，禁止定时兜底轮询 |
| 8 | 单一写入方 | 排序 / 筛选 / 视图由 UI 回调独占写，consumer 只读 |
| 9 | 高频 Msg 不逐条 invoke | 分批 drain + 分档节流（8 / 120 / 250ms） |
| 10 | 错误处理 | Docker 错误统一转 `Result<T, String>`，UI 展示中文文本 |

红线：架构优先于实现（R3）、复用率 ≥ 80%（R4）、单文件正文 ≤ 300 行（R2）、项目名禁止出现废弃旧名（R0）。

## 9. 文档同步与 Wiki 双站

改了源码结构要同步 README 项目结构树、AGENTS.md 模块地图、docs/architecture.md；改了用户可见功能要同步 CHANGELOG.md、README、AGENTS.md 已知限制与 Wiki Home / Usage；发版要同步四处版本号（Cargo.toml / packaging/debian/control / Cargo.lock / CHANGELOG.md）。

Wiki 是 GitHub 与 Gitee 两个独立仓库，**任何变更必须同一批改动同时推两站**，禁止只更新一侧；两站内容一致。内链一律用标准 Markdown 相对链接（如 `[安装](Installation)`），不用 Gollum 的 MediaWiki 双链语法（Gitee CommonMark 不支持）；图片走 jsDelivr CDN，Wiki 仓库不放二进制图片。本仓库 `wiki/` 六页即双站 Wiki 的规范源，六页为 Home / Installation / Usage / FAQ / Changelog / Contributing。

## 10. 翻译与沟通

界面文案用 gettext，七语言为简体中文 / 英语（msgid 源串，无 en 目录）/ 西班牙语 / 法语 / 德语 / 俄语 / 葡萄牙语，locale 目录为 zh_CN / es_ES / fr_FR / de_DE / ru_RU / pt_PT；缺失译文回落英文源串。新增语言按仓库 docs/translations.md 走。

缺陷与功能建议走 [GitHub Issues](https://github.com/pengtang1024/docker-panel/issues) 或 [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues)；提问前看 SUPPORT.md；安全漏洞走私用渠道（SECURITY.md），不要发公开 Issue。版本改了什么见 [Changelog](Changelog)，排障见 [FAQ](FAQ)。

## 页面导航

[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)
