//! 界面文案翻译（Slint @tr 与 Rust t/trf/trfp 共用 translations/<locale>/LC_MESSAGES/docker-panel.po）
//!
//! - 配置文件是唯一文案源：Slint 编译器在 build.rs 阶段把 .po 嵌入二进制，
//!   本模块通过构建脚本生成的常量表读取同一批文件，运行时按需查表，零额外依赖。
//! - 支持七语言：简体中文（zh_CN）、英语（msgid 源串，无 en 文件）、西班牙语（es_ES）、
//!   法语（fr_FR）、德语（de_DE）、俄语（ru_RU）、葡萄牙语（pt_PT）；**不支持日语**。
//! - 语言检测：LC_ALL > LC_MESSAGES > LANG，按 locale 归一化到上述目录名；
//!   只识别语言部分（如 es_MX → es_ES），缺失翻译回落英文 msgid 源串。
//! - 占位符约定：`{0}` `{1}` …，与 .slint 中 @tr 的编号保持一致。

use std::collections::HashMap;
use std::sync::OnceLock;

// build.rs 生成：本机构建时实际存在的 (locale, .po 内容) 列表。
// 受支持 locale 的 .po 尚未落地时，该项为空串（运行时回落英文）。
include!(concat!(env!("OUT_DIR"), "/i18n_catalogs.rs"));

/// 受支持的非英语 locale；顺序与生成常量表一致，英语由 msgid 源串直接承担。
pub const SUPPORTED_LOCALES: &[&str] = &["zh_CN", "es_ES", "fr_FR", "de_DE", "ru_RU", "pt_PT"];

/// 运行时选中的 locale：始终是 SUPPORTED_LOCALES 之一或 "en"。
static LOCALE: OnceLock<&'static str> = OnceLock::new();
/// 对应 locale 的 msgid → msgstr 表；英语为 None（直接返回 msgid）。
static TABLE: OnceLock<Option<HashMap<String, String>>> = OnceLock::new();

/// 启动时调用：检测系统语言并初始化查表。
pub fn init() {
    let locale = detect_locale();
    let table = if locale == "en" {
        None
    } else {
        CATALOGS
            .iter()
            .find(|(name, po)| *name == locale && !po.is_empty())
            .map(|(_, po)| parse_po(po))
    };
    let _ = LOCALE.set(if table.is_some() { locale } else { "en" });
    let _ = TABLE.set(table);
}

/// Slint bundled translations 需要的语言代码：英文返回 None（Slint 使用 msgid 原串）。
pub fn slint_language() -> Option<&'static str> {
    match LOCALE.get().copied().unwrap_or("en") {
        "en" => None,
        locale => Some(locale),
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
/// 按当前 locale 的 Plural-Forms 从 msgstr[n] 选形式；无译文时用英文 `n == 1 ? 单数 : 复数`。
pub fn trfp(singular: &str, plural: &str, n: usize, args: &[&str]) -> String {
    let base = match (TABLE.get(), locale_has_plural_forms()) {
        (Some(Some(map)), true) => map
            .get(&format!("{singular}\x00{plural}"))
            .and_then(|forms| forms.split('\x00').nth(plural_index(n)))
            .map(str::to_owned)
            .unwrap_or_else(|| fallback_plural(singular, plural, n)),
        _ => fallback_plural(singular, plural, n),
    };
    fill(base, args)
}

/// 英文缺省复数规则；与 gettext 默认 `nplurals=2; plural=(n != 1);` 一致。
fn fallback_plural(singular: &str, plural: &str, n: usize) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}

/// 当前 locale 的 .po 是否声明了 Plural-Forms（没有则按英文规则）。
fn locale_has_plural_forms() -> bool {
    let locale = LOCALE.get().copied().unwrap_or("en");
    CATALOGS
        .iter()
        .find(|(name, _)| *name == locale)
        .is_some_and(|(_, po)| po.contains("\"Plural-Forms: "))
}

/// 当前 locale 的 plural 函数下标：zh_CN 取 0；fr_FR 取 (n > 1)；
/// ru_RU 取三态规则；es_ES / de_DE / pt_PT 取 (n != 1)。
fn plural_index(n: usize) -> usize {
    match LOCALE.get().copied().unwrap_or("en") {
        "zh_CN" => 0,
        "fr_FR" => usize::from(n > 1),
        "ru_RU" => russian_plural(n),
        _ => usize::from(n != 1),
    }
}

/// 俄语三态复数规则（下标 0/1/2）。
fn russian_plural(n: usize) -> usize {
    let last = n % 10;
    let last2 = n % 100;
    if last == 1 && last2 != 11 {
        0
    } else if (2..=4).contains(&last) && !(12..=14).contains(&last2) {
        1
    } else {
        2
    }
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

/// 检测系统 locale：LC_ALL > LC_MESSAGES > LANG，取第一个非空值；
/// `C` / `POSIX` / 空值视为英文。返回原始 locale（如 "zh_CN.UTF-8"）。
fn locale_from_env() -> &'static str {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim();
            if !v.is_empty() && v != "C" && v != "POSIX" {
                // 泄漏一次即可，进程生命周期内不变
                return Box::leak(v.to_string().into_boxed_str());
            }
        }
    }
    "en"
}

/// 将系统 locale 归一化到受支持目录名：
/// 先精确匹配（去编码/修饰符），再按语言部分匹配（如 es_MX → es_ES）；
/// 不支持的语言（含 ja/jp）与未匹配上的一律英文。
fn detect_locale() -> &'static str {
    let raw = locale_from_env();
    let normalized: String = raw
        .split(['.', '@'])
        .next()
        .unwrap_or("en")
        .replace('-', "_")
        .to_ascii_lowercase();
    for locale in SUPPORTED_LOCALES {
        if normalized == locale.to_ascii_lowercase() {
            return locale;
        }
    }
    let language = normalized.split('_').next().unwrap_or("");
    SUPPORTED_LOCALES
        .iter()
        .find(|locale| locale.split('_').next().is_some_and(|l| l == language))
        .copied()
        .unwrap_or("en")
}

/// 极简 .po 解析：支持 msgid/msgstr/msgid_plural/msgstr[n] 多行条目；
/// msgstr[n] 按序号存入以 \x00 分隔的串，供 trfp 按复数规则选择。
#[allow(unused_assignments)] // flush!() 在文件末尾最后一次调用后，fuzzy/state 重置不再被读取
fn parse_po(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut fuzzy = false;
    let mut msgid = String::new();
    let mut msgid_plural = String::new();
    let mut msgstr = String::new();
    let mut plural_forms: Vec<(usize, String)> = Vec::new();
    // state: 0=idle 1=msgid 2=msgid_plural 3=msgstr(含 [n])
    let mut state = 0u8;
    let mut has_plural = false;

    macro_rules! flush {
        () => {{
            let is_fuzzy = fuzzy;
            if !msgid.is_empty() && !is_fuzzy {
                if has_plural {
                    let mut forms: Vec<(usize, String)> = plural_forms.clone();
                    forms.sort_by_key(|(idx, _)| *idx);
                    let values: Vec<String> = forms.into_iter().map(|(_, text)| text).collect();
                    map.insert(format!("{msgid}\x00{msgid_plural}"), values.join("\x00"));
                } else {
                    map.entry(msgid.clone()).or_insert_with(|| msgstr.clone());
                }
            }
            msgid.clear();
            msgid_plural.clear();
            msgstr.clear();
            plural_forms.clear();
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
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            // msgstr[0] "..."：序号决定复数形式下标
            if let Some((idx, _)) = rest.split_once(']') {
                if let Ok(idx) = idx.trim().parse::<usize>() {
                    plural_forms.push((idx, extract_quoted(rest)));
                }
            }
            state = 3;
        } else if line.starts_with("msgstr") {
            msgstr = extract_quoted(line);
            state = 3;
        } else if line.starts_with('"') {
            // 上一字符串的续行
            let content = parse_po_string(line);
            match state {
                1 => msgid.push_str(&content),
                2 => msgid_plural.push_str(&content),
                3 if plural_forms.last().is_some() => {
                    if let Some(last) = plural_forms.last_mut() {
                        last.1.push_str(&content);
                    }
                }
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
