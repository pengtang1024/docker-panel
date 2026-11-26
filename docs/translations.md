# 翻译与 i18n

> **一句话**：给要改界面文案、补翻译或新增语言的贡献者——界面文案全部走 gettext，支持七语言（简体中文、英语、西班牙语、法语、德语、俄语、葡萄牙语），缺翻译自动回落英文 msgid。

**对应源码**：`translations/<locale>/LC_MESSAGES/docker-panel.po`、`src/i18n.rs`、`src/log_status.rs`、`build.rs`｜**相关页面**：[README](../README.md)（功能与已知限制）

## 0. 本页导航

**结论**：九节覆盖从现状、读取路径到新增语言的完整链路，先按表定位。

| # | 章节 | 你会在这里找到 |
|:--:|------|----------------|
| 1 | [现状](#1-现状) | 七语言清单、文件格式、编译方式 |
| 2 | [工作原理](#2-工作原理) | Slint / Rust 两条读取路径与语言检测 |
| 3 | [`.po` 文件结构](#3-po-文件结构) | 文件头、分区注释、条目写法 |
| 4 | [维护规则](#4-维护规则) | 新增条目、占位符、缺失回落 |
| 5 | [占位符与复数](#5-占位符与复数) | 三个 API 与单复数槽位 |
| 6 | [新增一门语言](#6-新增一门语言) | 七语言清单、建目录、复数规则 |
| 7 | [日志状态提示文案](#7-日志状态提示文案) | 4 条 i18n key 的唯一定义处 |
| 8 | [Slint 已知限制](#8-slint-已知限制) | 翻译初始化的调用时机 |
| 9 | [相关文档](#9-相关文档) | 上下游页面 |

## 1. 现状

**结论**：界面支持**七语言**；英文不是单独的翻译文件，而是源码里的 msgid 源串本身，没有翻译的条目自动回落英文。

| # | 语言 | locale 目录 / 代码 | 翻译文件 | 复数规则（`Plural-Forms`） |
|:--:|------|--------------------|----------|----------------------------|
| 1 | 简体中文 | `zh_CN` | `translations/zh_CN/LC_MESSAGES/docker-panel.po`（264 msgid） | `nplurals=1; plural=0` |
| 2 | 英语 | 无（msgid 源串） | 无 `.po` 文件，源串即译文 | `nplurals=2; plural=(n != 1)`（Slint 默认） |
| 3 | 西班牙语 | `es_ES` | `translations/es_ES/LC_MESSAGES/docker-panel.po` | `nplurals=2; plural=(n != 1)` |
| 4 | 法语 | `fr_FR` | `translations/fr_FR/LC_MESSAGES/docker-panel.po` | `nplurals=2; plural=(n > 1)` |
| 5 | 德语 | `de_DE` | `translations/de_DE/LC_MESSAGES/docker-panel.po` | `nplurals=2; plural=(n != 1)` |
| 6 | 俄语 | `ru_RU` | `translations/ru_RU/LC_MESSAGES/docker-panel.po` | `nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<12 || n%100>14) ? 1 : 2)` |
| 7 | 葡萄牙语 | `pt_PT` | `translations/pt_PT/LC_MESSAGES/docker-panel.po` | `nplurals=2; plural=(n != 1)` |

| 项 | 值 |
|----|-----|
| 文件格式 | gettext `.po`，UTF-8；文件头 `Language` 字段必须与目录名逐字一致 |
| 编译方式 | **编译期嵌入二进制**（不读外部文件，运行时零 IO） |
| 语言代码 | 目录名 = 文件头 `Language` = `slint::select_bundled_translation()` 实参；严格使用上表代码 |

## 2. 工作原理

**结论**：Slint 与 Rust 两条读取路径共用**同一份 `.po` 文件**，都在编译期嵌入二进制，运行时零 IO。

| 路径 | 做法 |
|------|------|
| **Slint 侧** | `build.rs` 调 `slint_build::CompilerConfiguration::new().with_bundled_translations("translations")`，把 `.po` 编译进二进制；Slint 的 `@tr("...")` 属性按当前语言查出译文 |
| **Rust 侧** | `build.rs` 扫描上表六个 locale 的 `.po`（存在才嵌入，缺失用空串占位），生成 `OUT_DIR/i18n_catalogs.rs`；`src/i18n.rs` 据此把译文解析成 `HashMap<msgid, msgstr>`，运行时按需查表（零额外依赖，不引入 gettext 运行时库） |

语言检测顺序（`src/i18n.rs::locale_from_env`）：`LC_ALL` → `LC_MESSAGES` → `LANG`，取第一个非空值；`C` / `POSIX` / 空值视为英文。`detect_locale()` 去掉 `.编码` / `@修饰符`、把 `-` 归一为 `_`，先精确匹配六个 locale（大小写不敏感），再按语言部分匹配（如 `es_MX.UTF-8` → `es_ES`、`pt_BR` → `pt_PT`）；未支持语言与缺译文一律回落英文。

`i18n::slint_language()` 返回当前生效的 locale（如 `Some("fr_FR")`），英文返回 `None`（Slint 使用 msgid 原串）。`main.rs` 在 `MainWindow::new()` **之后**调用 `slint::select_bundled_translation(lang)`；Slint 按目录名精确匹配 locale，选不到时这里会打印错误并继续用 msgid 英文，不会崩溃。

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
| Slint 新增文案 | 文件中新增 `@tr("...")` 后，**必须**在**全部六个** `.po` 文件追加同名 `msgid` 条目；否则该语言回落英文 |
| Rust 动态消息 | msgid 用英文，占位符统一写成 `{0}` / `{1}`…，**译文必须原样保留占位符** |
| 未翻译条目 | 自动回落英文源串（所以「缺失」不会崩溃，只会显示英文） |

## 5. 占位符与复数

**结论**：译文里的占位符必须与 msgid 逐字一致，单复数形式由 `.po` 的 `Plural-Forms` 决定。

| API | 用途 | 用法 |
|-----|------|------|
| `i18n::t(msgid)` | 普通翻译 | `t("Connected")` |
| `i18n::trf(msgid, args)` | 带占位符 | msgid `"Removed {0}"`，`trf("Removed {0}", &[&name])` |
| `i18n::trfp(singular, plural, n, args)` | 复数 | 按 `n` 与 `.po` 的 `Plural-Forms` 选单复数形式 |

Slint 侧的复数用 `@tr("...")` 的复数语法（单数 | 复数 + `%n`），按各 `.po` 的 `Plural-Forms` 选择槽位。Rust 侧 `trfp` 按下表选择 `msgstr[n]`：**简体中文**固定 `msgstr[0]`；**英语 / 西语 / 德语 / 葡语** `n != 1` 选 `msgstr[1]`；**法语** `n > 1` 选 `msgstr[1]`（0 和 1 均用单数）；**俄语**三态：`n%10==1 && n%100!=11` → `[0]`，`n%10∈[2,4] && n%100∉[12,14]` → `[1]`，其余 → `[2]`。每个复数条目必须补齐该语言声明的全部槽位，缺槽位时退回第一个译文；占位符原样保留。

> [!IMPORTANT]
> 占位符是唯一不能"意译"的东西：译文里少一个 `{0}` 会直接丢参数，多一个则会显示字面量。

## 6. 七语言清单与新增一门语言

**结论**：当前七语言为简体中文（`zh_CN`）、英语（`msgid` 源串，**不建 `en` 文件**）、西班牙语（`es_ES`）、法语（`fr_FR`）、德语（`de_DE`）、俄语（`ru_RU`）、葡萄牙语（`pt_PT`）。

**语种代码铁律**：目录名 `translations/<locale>/LC_MESSAGES/docker-panel.po`、`.po` 头的 `Language` 字段、`slint::select_bundled_translation()` 实参三者必须**逐字一致**，严格使用上表代码（下划线形式，与 `zh_CN` 同风格）。

### 6.1 新语言落地步骤

以 `de_DE`（德语）为例：

```bash
# 1. 建目录（目录名 = 上表的 locale 代码）
mkdir -p translations/de_DE/LC_MESSAGES

# 2. 复制简体中文模板，逐条翻译 msgstr（保留 msgid / 占位符原样）
cp translations/zh_CN/LC_MESSAGES/docker-panel.po \
   translations/de_DE/LC_MESSAGES/docker-panel.po

# 3. 改头部字段：Language: de_DE，Plural-Forms: nplurals=2; plural=(n != 1);
#    复数条目补齐 msgstr[0]（单数）/ msgstr[1]（复数）
```

| 改动点 | 结论 |
|--------|------|
| `build.rs` | **无需改动**。`with_bundled_translations("translations")` 扫描整个目录纳入 Slint 侧；本模块的目录表已列出六个非英语 locale，新文件落地即会被 `include_str!` 编译进二进制（缺失时为空串 → 回落英文） |
| `src/i18n.rs` | **七语言范围内无需改动**。`SUPPORTED_LOCALES` 已列出六个非英语 locale，`plural_index()` 已覆盖六种复数规则；新增**超出七语言**的语种才需要同步扩展这两处 |
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
| [translations/](../translations/) | 七语言目录（英语无文件） |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [故障排查](troubleshooting.md) | [README 文档地图](../README.md#文档地图) | [架构设计](architecture.md) |
