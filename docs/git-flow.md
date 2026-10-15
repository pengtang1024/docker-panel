# Git Flow 协作规范

Docker Panel 采用 [git flow](https://nvie.com/posts/a-successful-git-branching-model/) 分支模型。
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
# 1. 从 develop 切出发布分支
git flow release start 0.1.1

# 2. 同步版本号（两处必须一致）
#    Cargo.toml 的 version 字段
#    packaging/debian/control 的 Version 字段
#    CHANGELOG.md 新增对应版本段落

# 3. 提交发版准备改动
git commit -am "chore: 发版 0.1.1"

# 4. 完成发布：合入 master、打 tag 0.1.1、回灌 develop
git flow release finish 0.1.1

# 5. 推送（tag 必须推，否则 Release 流水线不会触发）
git push origin master develop
git push origin 0.1.1
```

`git flow release finish` 会询问 tag 说明，直接回车使用默认的 `release: vX.Y.Z` 即可。

### 版本号同步的三个位置

| 文件 | 字段 |
|------|------|
| `Cargo.toml` | `version` |
| `packaging/debian/control` | `Version` |
| `CHANGELOG.md` | `## [X.Y.Z]` 段落与底部链接 |

漏掉任何一处都会导致 deb 包里的版本号与实际不符，或更新日志断档。

## 线上紧急修复

```bash
git flow hotfix start 0.1.2      # 从 master 切出
# 修复并提交
git flow hotfix finish 0.1.2     # 合入 master + 回灌 develop + 打 tag
git push origin master develop && git push origin 0.1.2
```

hotfix 同样会触发 Release 流水线，产出补丁版本。

## 流水线

仓库只保留一条工作流 [`.github/workflows/release.yml`](../.github/workflows/release.yml)，
仅在推送 `X.Y.Z` tag 时触发：编译 → 打包 deb → 生成 sha256 → 创建 GitHub Release。

不在 `push` 或 `pull_request` 上跑 CI，也不启用 Dependabot 自动开 PR。

## 常见错误

| 现象 | 成因 | 正解 |
|------|------|------|
| master 上出现 `merge: develop 合入 master` | 直接把 develop 合进了 master | 走 `release` 分支，由 `release finish` 中转 |
| 推了 tag 但没有 Release | 只推了分支没推 tag | `git push origin X.Y.Z` |
| deb 包版本号不对 | 只改了 Cargo.toml | 三处版本号一起改 |
| finish 报「无标签说明」 | tag 说明输入为空 | 回车用默认说明，或输入说明文字 |