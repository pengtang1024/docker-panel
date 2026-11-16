# 翻译与 i18n

> **一句话**：给要改界面文案、补翻译或新增语言的贡献者——界面文案全部走 gettext，当前只有简体中文，缺翻译自动回落英文 msgid。

**对应源码**：`translations/zh_CN/LC_MESSAGES/docker-panel.po`、`src/i18n.rs`、`src/log_status.rs`、`build.rs`｜**相关页面**：[README](../README.md)（功能与已知限制）

## 0. 本页导航

**结论**：九节覆盖从现状、读取路径到新增语言的完整链路，先按表定位。

| # | 章节 | 你会在这里找到 |
|:--:|------|----------------|
| 1 | [现状](#1-现状) | 语言、条目数、文件格式、编译方式 |
| 2 | [工作原理](#2-工作原理) | Slint / Rust 两条读取路径与语言检测 |
| 3 | [`.po` 文件结构](#3-po-文件结构) | 文件头、分区注释、条目写法 |
| 4 | [维护规则](#4-维护规则) | 新增条目、占位符、缺失回落 |
| 5 | [占位符与复数](#5-占位符与复数) | 三个 API 与单复数槽位 |
| 6 | [新增一门语言](#6-新增一门语言) | 建目录、复制模板、需要改的代码 |
| 7 | [日志状态提示文案](#7-日志状态提示文案) | 4 条 i18n key 的唯一定义处 |
| 8 | [Slint 已知限制](#8-slint-已知限制) | 翻译初始化的调用时机 |
| 9 | [相关文档](#9-相关文档) | 上下游页面 |

## 1. 现状

**结论**：只有**简体中文**一门翻译；英文不是单独的翻译文件，而是源码里的 msgid 源串本身，没有翻译的条目自动回落英文。

| 项 | 值 |
|----|-----|
| 语言目录 | `translations/zh_CN/LC_MESSAGES/docker-panel.po` |
| msgid 条目 | 264 条 |
| 复数条目（`msgid_plural`） | 1 条 |
| 文件格式 | gettext `.po`，UTF-8，`Plural-Forms: nplurals=1; plural=0` |
| 编译方式 | **编译期嵌入二进制**（不读外部文件，运行时零 IO） |

## 2. 工作原理

**结论**：Slint 与 Rust 两条读取路径共用**同一份 `.po` 文件**，都在编译期嵌入二进制，运行时零 IO。

| 路径 | 做法 |
|------|------|
| **Slint 侧** | `build.rs` 调 `slint_build::CompilerConfiguration::new().with_bundled_translations("translations")`，把 `.po` 编译进二进制；Slint 的 `@tr("...")` 属性按当前语言查出译文 |
| **Rust 侧** | `src/i18n.rs` 用 `include_str!("../translations/zh_CN/LC_MESSAGES/docker-panel.po")` 读同一文件，启动时解析成 `HashMap<msgid, msgstr>`，运行时按需查表（零额外依赖，不引入 gettext 运行时库） |

语言检测顺序（`src/i18n.rs::detect_lang`）：`LC_ALL` → `LC_MESSAGES` → `LANG`，取第一个非空值；**以 `zh` 开头**启用简体中文，其它一律回落英文源串。

`i18n::slint_language()` 只在中文时返回 `Some("zh_CN")`，其它语言返回 `None`（Slint 使用 msgid 原串）。

## 3. `.po` 文件结构

**结论**：文件用 `# ===== 区域名 =====` 的注释分段，新增条目必须放进对应分区，文件头保持完整。

```po
# docker-panel 简体中文翻译
msgid ""
msgstr ""
"Project-Id-Version: docker-panel\n"
"Language: zh_CN\n"
"MIME-Version: 1.0\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"
"Plural-Forms: nplurals=1; plural=0;\n"

# ===== 顶栏 =====
msgid "Connecting..."
msgstr "正在连接…"

msgid "Connected"
msgstr "已连接"
```

分区包括顶栏 / 筛选栏 / 台账 / 抽屉 / 日志 / 弹窗 / 设置等。

## 4. 维护规则

**结论**：`.po` 文件头记录的三条规则是硬要求，少一条就会漏译或丢参数。

| 规则 | 要求 |
|------|------|
| Slint 新增文案 | 文件中新增 `@tr("...")` 后，**必须**在此文件追加同名 `msgid` 条目；否则该文案没有中文译文 |
| Rust 动态消息 | msgid 用英文，占位符统一写成 `{0}` / `{1}`…，**译文必须原样保留占位符** |
| 未翻译条目 | 自动回落英文源串（所以「缺失」不会崩溃，只会显示英文） |

## 5. 占位符与复数

**结论**：译文里的占位符必须与 msgid 逐字一致，单复数形式由 `.po` 的 `Plural-Forms` 决定。

| API | 用途 | 用法 |
|-----|------|------|
| `i18n::t(msgid)` | 普通翻译 | `t("Connected")` |
| `i18n::trf(msgid, args)` | 带占位符 | msgid `"Removed {0}"`，`trf("Removed {0}", &[&name])` |
| `i18n::trfp(singular, plural, n, args)` | 复数 | 按 `n` 与 `.po` 的 `Plural-Forms` 选单复数形式 |

Slint 侧的复数用 `@tr("...")` 的复数语法（单数 | 复数 + `%n`）。**中文 `nplurals=1`**，复数与单数译文相同；新增语言若有多个复数形式，需相应修改该语言的 `Plural-Forms` 并补齐 `msgstr[0]` / `msgstr[1]` 等槽位。

> [!IMPORTANT]
> 占位符是唯一不能"意译"的东西：译文里少一个 `{0}` 会直接丢参数，多一个则会显示字面量。

## 6. 新增一门语言

**结论**：复制目录与模板即可，`build.rs` 不用改；但 `src/i18n.rs` 目前硬编码 zh_CN，必须同步扩展。

以新增 `de`（德语）为例：

```bash
# 1. 建目录（目录名用 POSIX locale 的 language_COUNTRY 或 language 形式）
mkdir -p translations/de/LC_MESSAGES

# 2. 复制现有文件作为模板
cp translations/zh_CN/LC_MESSAGES/docker-panel.po translations/de/LC_MESSAGES/docker-panel.po

# 3. 改头部 Language 字段，逐条替换 msgstr
#    "Language: de\n"
```

| 改动点 | 结论 |
|--------|------|
| `build.rs` | **无需改动**。`with_bundled_translations("translations")` 会扫描整个目录，新语言目录自动纳入 Slint 侧 |
| `src/i18n.rs` | **需要改动**——这是当前实现的真实限制：`const ZH_PO: &str = include_str!("../translations/zh_CN/LC_MESSAGES/docker-panel.po")` 硬编码了 zh_CN 一份，`detect_lang()` 也只判 `zh*`。新增语言必须把这两处扩成按 locale 选择（或在 PR 中标注由维护者补齐） |
| 验证 | `LANG=de_DE.UTF-8 cargo run`，检查界面文案是否切换；未翻译条目应显示英文而不是空白 |

**翻译 PR 请说明**：目标 locale、覆盖的 msgid 条数（对照 264 条的完成度）、占位符是否全部保留、是否已实测启动验证。

## 7. 日志状态提示文案

**结论**：`src/log_status.rs` 是这 4 条提示的**唯一定义处**，其它文件只能经它取译文。

| 常量 | 中文文案 |
|------|---------|
| `NOTICE_INTERRUPTED` | 日志流中断，正在重连… |
| `NOTICE_RECOVERED` | 日志流已恢复连接 |
| `NOTICE_STOPPED` | 日志流已停止 |
| `NOTICE_SAMPLED` | 日志洪峰：已按比例抽样显示，省略 N 行 |

禁止在其它文件内联这些中文字面量——它们必须经由该模块取译文，否则换语言时会漏译。

## 8. Slint 已知限制

**结论**：翻译初始化有严格的调用顺序，放错位置会恒报错。

> [!WARNING]
> `select_bundled_translation` **必须在首个组件创建之后调用**（即 `MainWindow::new()` 之后）。在它之前调用时全局上下文尚未初始化，会恒报 `NoTranslationsBundled`（debug / release 行为一致）。切换语言后由 `translations_dirty` 自动重算 `@tr` 属性。

## 9. 相关文档

**结论**：改文案与提 PR 看前两项，查实现看后两项。

| 文档 | 内容 |
|------|------|
| [CONTRIBUTING.md](../CONTRIBUTING.md) | 贡献流程与 PR 要求 |
| [AGENTS.md](../AGENTS.md) | 技术栈与 Slint 1.18 已验证限制 |
| [src/i18n.rs](../src/i18n.rs) | 运行时查表实现 |
| [translations/zh_CN/LC_MESSAGES/docker-panel.po](../translations/zh_CN/LC_MESSAGES/docker-panel.po) | 现有翻译文件 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [故障排查](troubleshooting.md) | [README 文档地图](../README.md#文档地图) | [架构设计](architecture.md) |
