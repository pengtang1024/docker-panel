提交前请先阅读 `CONTRIBUTING.md`。

## 改动说明

用一两句话说明本次改动解决什么问题、改完带来什么变化，再按需列出主要变更点。

## 改动类型

- [ ] `feat` 新功能
- [ ] `fix` 缺陷修复
- [ ] `perf` 性能
- [ ] `refactor` 重构
- [ ] `docs` 文档
- [ ] `chore` 杂项
- [ ] `ci` 流水线

## 关联 Issue

用 `Closes #123` 形式关联；多个 Issue 分行写，无关联时写「无」。

## 验证

以下四条命令在本地跑通后再勾选：

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test`
- [ ] `cargo build --release`

## UI 改动

涉及界面时必须附 `agkit shot --app docker-panel -o /tmp/dp.png` 的截图（本项目 UI 验收硬约束）；不涉及界面则填「不涉及」。

## 文档同步

- [ ] README 项目结构树
- [ ] AGENTS.md 模块地图
- [ ] CHANGELOG.md
- [ ] Wiki 两站同步（GitHub + Gitee）

## 自检

- [ ] 项目旧称零残留：`grep -riE 'd[v]kit' .` 无输出（旧称禁令见 AGENTS.md 顶部硬约束；方括号写法避免检查命令自身命中）
- [ ] 源码文件去除头部与注释后正文 ≤ 300 行

PR 目标分支一律 `develop`。
