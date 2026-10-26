# 许可证与第三方组件

本文件记录 docker-panel 的许可证选择与义务边界。**`LICENSE` 只覆盖本项目自有代码，不覆盖第三方组件。**

## 1. 自有代码

MIT，全文见 [`LICENSE`](LICENSE)。适用于 `src/`、`ui/`、`translations/`、`docs/`、`scripts/`、`packaging/` 等本仓库原创文件。

## 2. UI 框架 Slint

| 项目 | 内容 |
|------|------|
| 组件 | `slint` / `i-slint-*` 1.18.1 |
| 上游许可 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` |
| **本项目选择** | **`LicenseRef-Slint-Royalty-free-2.0`** |
| 全文 | [`licenses/Slint-Royalty-free-2.0.md`](licenses/Slint-Royalty-free-2.0.md) |

**义务（Royalty-free 2.0 §2）**：在 About 界面展示 `AboutSlint`，或在一张可下载二进制的公开网页展示 “Made with Slint” 徽章。本项目采用后者：README 顶部徽章 + GitHub Release 说明。本项目没有 About 界面，且未启用 `std-widgets` feature（引入 `AboutSlint` 会带入整套 Fluent 控件）。

**限制（§3）**：不得单独分发 Slint（仓库内不得保留 Slint 源码副本）；不得用于嵌入式系统；不得分发暴露 Slint API 的应用。

**未选择的两个分支**：

- `GPL-3.0-only`：若改用该分支，整个二进制以 GPLv3 分发，本项目的 MIT 宣称必须同步撤销，并提供完整对应源码。
- `LicenseRef-Slint-Software-3.0`：商业许可，需要按席位付费。

## 3. 第三方组件

完整组件清单、分组与许可文本索引见 [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)，全文放在 [`licenses/`](licenses/)。

## 4. 仓库内的第三方目录

- `material-1.1.0/`：Slint Material Components 1.1.0，**MIT**，Copyright (c) 2026 SixtyFPS GmbH，见 [`material-1.1.0/LICENSE.md`](material-1.1.0/LICENSE.md)。它会被编译进二进制，MIT 通知随发行物一起提供。
- 仓库内**不存在** Slint 源码副本。历史 `vendor/` 目录已删除；构建使用 crates.io 上的原版 crate。

## 5. 变更条件

以下任一情况发生时，必须同步更新本文件、`THIRD-PARTY-NOTICES.md` 与 `packaging/debian/copyright`：

- 升级 Slint 大版本或切换许可分支；
- 更换 UI 框架；
- 引入新的 copyleft / 非 OSI 依赖；
- 新增需要随二进制分发的第三方素材（字体、图标、数据）。
