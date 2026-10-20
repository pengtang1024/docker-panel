//! 界面文案翻译（与 Slint @tr 共用 translations/zh_CN/LC_MESSAGES/docker-panel.po）
//!
//! - 配置文件是唯一文案源：Slint 编译器在 build.rs 阶段把 .po 嵌入二进制，
//!   本模块通过 include_str! 读取同一份文件，运行时按需查表，零额外依赖。
//! - 语言选择：检测 LANG / LC_ALL / LC_MESSAGES，zh* → 简体中文；
//!   其它语言一律回落到英文 msgid 源串。
//! - 占位符约定：`{0}` `{1}` …，与 .slint 中 @tr 的编号保持一致。

use std::sync::OnceLock;

use std::collections::HashMap;

/// 内嵌的简体中文 .po（与 Slint bundled translations 指向同一文件）
const ZH_PO: &str = include_str!("../translations/zh_CN/LC_MESSAGES/docker-panel.po");

static TABLE: OnceLock<Option<HashMap<String, String>>> = OnceLock::new();
static LANG: OnceLock<&'static str> = OnceLock::new();

/// 启动时调用：检测系统语言并初始化查表。
pub fn init() {
    let lang = detect_lang();
    let _ = LANG.set(lang);
    TABLE.get_or_init(|| {
        if lang.starts_with("zh") {
            Some(parse_po(ZH_PO))
        } else {
            None
        }
    });
}

/// Slint bundled translations 需要的语言代码（非中文返回 None，保持英文源串）。
pub fn slint_language() -> Option<&'static str> {
    if LANG.get().copied().unwrap_or("en").starts_with("zh") {
        Some("zh_CN")
    } else {
        None
    }
}

/// 翻译普通字符串；args 按顺序替换 {0} {1} …
pub fn t(msgid: &str) -> String {
    translate(msgid).into_owned()
}

/// 翻译并替换占位符：`trf("Loaded {0} containers", &[&n.to_string()])`
pub fn trf(msgid: &str, args: &[&str]) -> String {
    fill(translate(msgid).into_owned(), args)
}

/// 复数翻译，对齐 .slint 的 `@tr("sing" | "plural" % n)`：
/// 中文 nplurals=1 取 msgstr[0]；非中文回落 `n == 1 ? singular : plural`。
pub fn trfp(singular: &str, plural: &str, n: usize, args: &[&str]) -> String {
    let base = match TABLE.get() {
        Some(Some(_)) => translate(&format!("{singular}\x00{plural}")).into_owned(),
        _ => {
            if n == 1 {
                singular.to_string()
            } else {
                plural.to_string()
            }
        }
    };
    fill(base, args)
}

/// 按 `{0}` `{1}` … 顺序替换占位符。
fn fill(mut s: String, args: &[&str]) -> String {
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

fn translate(msgid: &str) -> std::borrow::Cow<'_, str> {
    if let Some(Some(map)) = TABLE.get() {
        if let Some(v) = map.get(msgid) {
            return std::borrow::Cow::Borrowed(v.as_str());
        }
    }
    std::borrow::Cow::Borrowed(msgid)
}

/// 检测系统语言：LC_ALL > LC_MESSAGES > LANG，返回如 "zh_CN"、"en_US"，默认 en。
fn detect_lang() -> &'static str {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim();
            if !v.is_empty() && v != "C" && v != "POSIX" {
                // 泄漏一次即可，进程生命周期内不变
                return Box::leak(
                    v.split('.')
                        .next()
                        .unwrap_or("en")
                        .to_string()
                        .into_boxed_str(),
                );
            }
        }
    }
    "en"
}

/// 极简 .po 解析：只支持本项目用到的 msgid/msgstr/msgid_plural 多行条目。
#[allow(unused_assignments)] // flush!() 在文件末尾最后一次调用后，fuzzy/state 重置不再被读取
fn parse_po(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut fuzzy = false;
    let mut msgid = String::new();
    let mut msgid_plural = String::new();
    let mut msgstr = String::new();
    // state: 0=idle 1=msgid 2=msgid_plural 3=msgstr(含 [n])
    let mut state = 0u8;
    let mut has_plural = false;

    macro_rules! flush {
        () => {{
            let is_fuzzy = fuzzy;
            if !msgid.is_empty() && !is_fuzzy {
                if has_plural {
                    // 只取 msgstr[0]（中文 nplurals=1）
                    map.insert(format!("{msgid}\x00{msgid_plural}"), msgstr.clone());
                } else {
                    map.entry(msgid.clone()).or_insert_with(|| msgstr.clone());
                }
            }
            msgid.clear();
            msgid_plural.clear();
            msgstr.clear();
            has_plural = false;
            fuzzy = false;
            state = 0;
        }};
    }

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush!();
            continue;
        }
        if line.starts_with('#') {
            if line.contains("fuzzy") {
                fuzzy = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("msgid_plural ") {
            msgid_plural = parse_po_string(rest);
            has_plural = true;
            state = 2;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            if state != 0 {
                flush!();
            }
            msgid = parse_po_string(rest);
            state = 1;
        } else if line.starts_with("msgstr") {
            // msgstr "..." 或 msgstr[0] "..."：取最后一个引号对内的内容
            let content = extract_quoted(line);
            msgstr = content;
            state = 3;
        } else if line.starts_with('"') {
            // 上一字符串的续行
            let content = parse_po_string(line);
            match state {
                1 => msgid.push_str(&content),
                2 => msgid_plural.push_str(&content),
                3 => msgstr.push_str(&content),
                _ => {}
            }
        }
    }
    flush!();
    // 空 msgid 的 header 不应被当作翻译
    map.remove("");
    map
}

fn extract_quoted(line: &str) -> String {
    let start = match line.find('"') {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let body = &line[start..];
    let end = body.rfind('"').unwrap_or(body.len());
    unescape(&body[..end])
}

fn parse_po_string(token: &str) -> String {
    let token = token.trim();
    let start = token.find('"').map(|i| i + 1).unwrap_or(0);
    let end = token.rfind('"').unwrap_or(token.len());
    if start >= end {
        return String::new();
    }
    unescape(&token[start..end])
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
