# Git Flow 协作规范

docker-panel 采用 [git flow](https://nvie.com/posts/a-successful-git-branching-model/) 分支模型。
本文档记录具体约定，任何改动分支拓扑的操作都以此为准。

## 分支模型

| 分支 | 职责 | 允许的写入方式 |
|------|------|----------------|
| `master` | 已发布版本，只接受发布产物 | 仅 `git flow release finish` / `git flow hotfix finish` |
| `develop` | 集成分支，日常合并目标 | feature / bugfix / release / hotfix 合并进入 |
| `feature/*` | 新功能，从 `develop` 切出 | 合并回 `develop` 后删除 |
| `bugfix/*` | 缺陷修复，从 `develop` 切出 | 合并回 `develop` 后删除 |
| `release/*` | 发版准备，从 `develop` 切出 | `git flow release finish` 合入 `master` 并回灌 `develop` |
| `hotfix/*` | 线上紧急修复，从 `master` 切出 | `git flow hotfix finish` 合入 `master` 并回灌 `develop` |

## 铁律

1. **禁止直接合并 `develop` 到 `master`。**
   `master` 只能由 `git flow release finish` 或 `git flow hotfix finish` 写入。
   日常开发一律先合入 `develop`，发版时由 `release` 分支中转。

2. **禁止向 `master` 直接提交。**
   即使只有一个文件的改动，也必须走 `hotfix` 分支，否则发布历史会出现无法追溯的提交。

3. **`master` 的每次提交都必须对应一个 tag。**
   tag 由 git flow 在 finish 时创建，格式为纯数字 `X.Y.Z`（不带 `v` 前缀），
   与 Release 流水线的触发正则 `[0-9]+.[0-9]+.[0-9]+` 保持一致。

4. **`release` 分支只做发版准备，不接受新功能。**
   切出后只允许：版本号同步、更新 CHANGELOG、发布前修复。功能需求应回到 `feature` 分支。

5. **版本号遵循语义化版本。**
   - `MAJOR`：不兼容的架构或数据格式变更
   - `MINOR`：向后兼容的新功能
   - `PATCH`：向后兼容的缺陷修复

6. **发版前本地必须通过检查。**
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo build --release --locked
   ```
   CI 只保留Release 一条流水线，不做提交期检查，因此本地检查是唯一门禁。

## 发版流程

```bash
# 0. 发版前门禁（本地是唯一门禁，CI 不做提交期检查）
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release --locked

# 1. 从 develop 切出发布分支
git flow release start 0.1.1

# 2. 同步版本号 + 写版本更新日志（三处必须一致，缺一不可）
#    a) Cargo.toml 的 version 字段
#    b) packaging/debian/control 的 Version 字段
#    c) CHANGELOG.md：把 [Unreleased] 段落改写成 ## [X.Y.Z] - YYYY-MM-DD，
#       并在底部链接区补一行 [X.Y.Z]: .../releases/tag/X.Y.Z
#    c) 最容易漏 —— 漏了就出现「有 tag、没更新日志」的断档版本。
#       改完用 scripts/check-version.sh 核一遍三处是否一致。

# 3. 提交发版准备改动
git commit -am "chore: 发版 0.1.1"

# 4. 完成发布：合入 master、打 tag 0.1.1、回灌 develop
git flow release finish 0.1.1

# 5. 推送（tag 必须单独推：一次多 ref push 会吞掉 tag 事件，流水线不触发）
git push origin master develop
git push origin refs/tags/0.1.1
```

`git flow release finish` 会询问 tag 说明，直接回车使用默认的 `release: vX.Y.Z` 即可。

### 版本号同步的三个位置

| 文件 | 字段 | 改法 |
|------|------|------|
| `Cargo.toml` | `version` | 改成 `X.Y.Z` |
| `packaging/debian/control` | `Version` | 改成 `X.Y.Z` |
| `CHANGELOG.md` | `## [X.Y.Z]` 段落与底部链接 | `[Unreleased]` 段落**改写**为 `## [X.Y.Z] - YYYY-MM-DD`（内容按 新增/变更/修复/移除 分类），底部链接区补 `[X.Y.Z]: .../releases/tag/X.Y.Z` |

漏掉任何一处都会导致 deb 包里的版本号与实际不符，或更新日志断档。

核对命令：

```bash
scripts/check-version.sh            # 比较三处版本 + CHANGELOG 段落 + 当前 tag
```

## 线上紧急修复

hotfix 同样产出补丁版本，**同样要写版本更新日志**——热修不是"偷偷发个补丁"，
`CHANGELOG.md` 必须新增 `[X.Y.Z]` 段落，版本号三处同步不能省。

```bash
# 1. 从 master 切出热修分支
git flow hotfix start 0.1.2

# 2. 修复并提交
git commit -am "fix: ..."

# 3. 同步版本号 + 写版本更新日志（三处，与发版流程同）
#    a) Cargo.toml: version = "0.1.2"
#    b) packaging/debian/control: Version: 0.1.2
#    c) CHANGELOG.md: 新增 ## [0.1.2] - YYYY-MM-DD 段落 + 底部链接
git commit -am "chore: bump version v0.1.2 (three-way sync)"
scripts/check-version.sh

# 4. 门禁（同发版流程）
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release --locked

# 5. 合入 master + 回灌 develop + 打 tag
git flow hotfix finish 0.1.2

# 6. 推送（tag 单独推）
git push origin master develop
git push origin refs/tags/0.1.2
```

hotfix 同样会触发 Release 流水线，产出补丁版本。

## 流水线

仓库只保留一条工作流 [`.github/workflows/release.yml`](../.github/workflows/release.yml)，
仅在推送 `X.Y.Z` tag 时触发：编译 → 打包 deb → 生成 sha256 → 提取版本更新日志 → 创建 GitHub Release。

### Release 页面的更新日志

发版页面（`releases/tag/X.Y.Z`）的正文来自 **`CHANGELOG.md` 里该版本的完整段落**：

- 流水线用 `scripts/release-notes.sh X.Y.Z` 抽出 `## [X.Y.Z]` 段落写进 `release-notes.md`，
  再以 `body_path` 传给 `softprops/action-gh-release`；`generate_release_notes` 保留，
  GitHub 会把 `**Full Changelog**: <compare 链接>` 追加在后面。
- 段落缺失时流水线**直接失败**（`::error::`），避免发出一个没有更新日志的版本。
- 流水线修好之前发的版本，页面正文只有一行 `**Full Changelog**: ...`——发布后由
  「回填历史 Release 更新日志」步骤用同样的段落补齐（immutable release 改不了就跳过，不影响本次发布）。

本地预览某个版本的发布正文：

```bash
scripts/release-notes.sh 0.3.1
```

不在 `push` 或 `pull_request` 上跑 CI，也不启用 Dependabot 自动开 PR。

### CI 的系统依赖（改工作流前必读）

GitHub runner 镜像**不预装** fontconfig / freetype 的开发包，`cargo build` 会在
`yeslogic-fontconfig-sys` 的 build script 处 panic：
`The system library 'fontconfig' required by crate 'yeslogic-fontconfig-sys' was not found`。

工作流的 apt 列表必须包含 `libfontconfig1-dev`，它会连带装上
`libfreetype-dev` → `zlib1g-dev` / `libbz2-dev` / `libpng-dev` / `libbrotli-dev`，
缺任何一个都会导致 `fontconfig.pc` / `freetype2.pc` 的 `Requires` 解析失败。

本地复现该环境的方法（只暴露 CI 已安装的 `.pc`）：

```bash
mkdir -p /tmp/fake-pc && ln -sf /usr/lib/x86_64-linux-gnu/pkgconfig/{x11,xcb,wayland-client,wayland-cursor,wayland-egl,xkbcommon,gl,egl,glesv2}.pc /tmp/fake-pc/
PKG_CONFIG_LIBDIR=/tmp/fake-pc cargo build --release --locked   # 复现缺 fontconfig 的失败
```

## 常见错误

| 现象 | 成因 | 正解 |
|------|------|------|
| master 上出现 `merge: develop 合入 master` | 直接把 develop 合进了 master | 走 `release` 分支，由 `release finish` 中转 |
| 推了 tag 但没有 Release | 只推了分支没推 tag | `git push origin refs/tags/X.Y.Z` |
| 推了 tag 但 Release 流水线没触发 | tag 用了 `v` 前缀，不匹配正则 `[0-9]+.[0-9]+.[0-9]+` | 用纯数字 `X.Y.Z` |
| 一次 `git push --all` 带上了 tag，流水线没触发 | 同一批 push 的 tag 事件被吞 | tag 单独 push 一次（`git push origin refs/tags/X.Y.Z`） |
| Release 页面没有更新日志（只有一行 Full Changelog） | 流水线只用了 `generate_release_notes` | 用 `body_path` 传 `scripts/release-notes.sh` 抽出的 CHANGELOG 段落 |
| CI 在 `cargo build` 处失败且报 fontconfig 缺失 | apt 列表缺 `libfontconfig1-dev` | 见上文「CI 的系统依赖」 |
| Release 页面没有更新日志 / CHANGELOG 断档 | 漏改 `[Unreleased]` 段落 | 发版与 hotfix 都要把 `[Unreleased]` 改写为 `## [X.Y.Z]` 并补底部链接 |
| deb 包版本号不对 | 只改了 Cargo.toml | 三处版本号一起改（`scripts/check-version.sh` 可一键核对） |
| 推了 tag 但流水线没触发，重推后却跑出**两个** run，第二个报 `Cannot delete asset from an immutable release` | 第一次 run 已发布 Release 并把它设为 immutable；重推 tag 又触发一次 run，它想先删同名旧资产 → 被 immutable 拒绝 | 第二个 run 失败**不影响结果**（Release 与资产已由第一个 run 生成好）；排查时先确认 `gh release list` 里有没有该版本，再决定要不要重推。推 tag 前先 `git push origin refs/tags/X.Y.Z`，若 `gh run list` 查不到，等 1~2 分钟再重推（重推会留下两条流水线记录） |
| finish 报「无标签说明」 | tag 说明输入为空 | 回车用默认说明，或输入说明文字 |