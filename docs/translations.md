# 翻译与 i18n

docker-panel 的界面文案用 **gettext** 管理。当前只有**简体中文**一门语言；英文不是单独的翻译文件，而是源码里的 msgid 源串本身——没有翻译的条目自动回落英文。

## 现状

| 项 | 值 |
|----|-----|
| 语言目录 | `translations/zh_CN/LC_MESSAGES/docker-panel.po` |
| msgid 条目 | 264 条 |
| 复数条目（`msgid_plural`） | 1 条 |
| 文件格式 | gettext `.po`，UTF-8，`Plural-Forms: nplurals=1; plural=0` |
| 编译方式 | **编译期嵌入二进制**（不读外部文件，运行时零 IO） |

## 工作原理

两条读取路径共用**同一份 `.po` 文件**：

1. **Slint 侧**：`build.rs` 调 `slint_build::CompilerConfiguration::new().with_bundled_translations("translations")`，把 `.po` 编译进二进制；Slint 的 `@tr("...")` 属性按当前语言查出译文。
2. **Rust 侧**：`src/i18n.rs` 用 `include_str!("../translations/zh_CN/LC_MESSAGES/docker-panel.po")` 读同一文件，启动时解析成 `HashMap<msgid, msgstr>`，运行时按需查表（零额外依赖，不引入 gettext 运行时库）。

语言检测顺序（`src/i18n.rs::detect_lang`）：`LC_ALL` → `LC_MESSAGES` → `LANG`，取第一个非空值；**以 `zh` 开头**启用简体中文，其它一律回落英文源串。

`i18n::slint_language()` 只在中文时返回 `Some("zh_CN")`，其它语言返回 `None`（Slint 使用 msgid 原串）。

## `.po` 文件结构

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

文件用 `# ===== 区域名 =====` 的注释分段（顶栏 / 筛选栏 / 台账 / 抽屉 / 日志 / 弹窗 / 设置等），新增条目请放进对应分区。

## 维护规则

`.po` 文件头记录的三条规则是硬要求：

1. Slint 文件中新增 `@tr("...")` 后，**必须**在此文件追加同名 `msgid` 条目；否则该文案没有中文译文。
2. Rust 侧动态消息的 msgid 用英文，占位符统一写成 `{0}` / `{1}`…，**译文必须原样保留占位符**。
3. 无翻译的条目自动回落英文源串（所以「缺失」不会崩溃，只会显示英文）。

## 占位符与复数

| API | 用途 | 用法 |
|-----|------|------|
| `i18n::t(msgid)` | 普通翻译 | `t("Connected")` |
| `i18n::trf(msgid, args)` | 带占位符 | msgid `"Removed {0}"`，`trf("Removed {0}", &[&name])` |
| `i18n::trfp(singular, plural, n, args)` | 复数 | 按 `n` 与 `.po` 的 `Plural-Forms` 选单复数形式 |

Slint 侧的复数用 `@tr("...")` 的复数语法（单数 | 复数 + `%n`）。**中文 `nplurals=1`**，复数与单数译文相同；新增语言若有多个复数形式，需相应修改该语言的 `Plural-Forms` 并补齐 `msgstr[0]` / `msgstr[1]` 等槽位。

占位符是唯一不能"意译"的东西：译文里少一个 `{0}` 会直接丢参数，多一个则会显示字面量。

## 新增一门语言

以新增 `de`（德语）为例：

```bash
# 1. 建目录（目录名用 POSIX locale 的 language_COUNTRY 或 language 形式）
mkdir -p translations/de/LC_MESSAGES

# 2. 复制现有文件作为模板
cp translations/zh_CN/LC_MESSAGES/docker-panel.po translations/de/LC_MESSAGES/docker-panel.po

# 3. 改头部 Language 字段，逐条替换 msgstr
#    "Language: de\n"
```

关键点：

- **`build.rs` 无需改动**。`with_bundled_translations("translations")` 会扫描整个目录，新语言目录自动纳入 Slint 侧。
- **`src/i18n.rs` 需要改动**——这是当前实现的真实限制：`const ZH_PO: &str = include_str!("../translations/zh_CN/LC_MESSAGES/docker-panel.po")` 硬编码了 zh_CN 一份，`detect_lang()` 也只判 `zh*`。新增语言必须把这两处扩成按 locale 选择（或在 PR 中标注由维护者补齐）。
- 验证：`LANG=de_DE.UTF-8 cargo run`，检查界面文案是否切换；未翻译条目应显示英文而不是空白。

**翻译 PR 请说明**：目标 locale、覆盖的 msgid 条数（对照 264 条的完成度）、占位符是否全部保留、是否已实测启动验证。

## 日志状态提示文案

`src/log_status.rs` 是日志流状态提示文案的**唯一定义处**，共 4 条 i18n key：

| 常量 | 中文文案 |
|------|---------|
| `NOTICE_INTERRUPTED` | 日志流中断，正在重连… |
| `NOTICE_RECOVERED` | 日志流已恢复连接 |
| `NOTICE_STOPPED` | 日志流已停止 |
| `NOTICE_SAMPLED` | 日志洪峰：已按比例抽样显示，省略 N 行 |

禁止在其它文件内联这些中文字面量——它们必须经由该模块取译文，否则换语言时会漏译。

## Slint 已知限制

`select_bundled_translation` **必须在首个组件创建之后调用**（即 `MainWindow::new()` 之后）。在它之前调用时全局上下文尚未初始化，会恒报 `NoTranslationsBundled`（debug / release 行为一致）。切换语言后由 `translations_dirty` 自动重算 `@tr` 属性。

## 相关文档

- [CONTRIBUTING.md](../CONTRIBUTING.md) — 贡献流程与 PR 要求
- [AGENTS.md](../AGENTS.md) — 技术栈与 Slint 1.18 已验证限制
- [src/i18n.rs](../src/i18n.rs) — 运行时查表实现
- [translations/zh_CN/LC_MESSAGES/docker-panel.po](../translations/zh_CN/LC_MESSAGES/docker-panel.po) — 现有翻译文件
