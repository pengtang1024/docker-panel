//! Release 说明的 Markdown / HTML → 纯文本转换。
//!
//! 更新弹窗的说明区是纯文本渲染（`wrap: word-wrap`），而 GitHub Release 正文是
//! Markdown 还可能内嵌 HTML 徽标。直接展示会出现 `##`、`**`、反引号、`<a><img>`
//! 等原始标记，视觉上就是「排版乱」。这里在写入 `UpdateInfo` 之前统一清理：
//!
//! - 删除 HTML 标签与纯图片行（Made-with-Slint 徽标等）；
//! - `[文本](链接)` 只留文本（文本为空或与链接相同则留链接）；
//! - 去掉加粗 / 行内代码标记、标题的 `#`、引用的 `>`；
//! - 列表项 `- ` / `* ` 转成 `• `；分隔线（`---` 等）整行删除；
//! - `[0.6.2] - 2026-10-28` 这类 changelog 版本头剥掉方括号；
//! - 连续空行折叠为单个空行。
//!
//! 不追求完整 Markdown 语义，只保证「看起来干净、信息不丢」。

/// 去掉成对出现的标记（`**粗体**`、`` `代码` `` 等）。
fn remove_pair(s: &str, marker: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find(marker) {
        out.push_str(&rest[..p]);
        rest = &rest[p + marker.len()..];
    }
    out.push_str(rest);
    out
}

/// 删除所有 `<...>` 形式的 HTML 标签。
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 删除 `![alt](url)` 图片引用（徽标图片在纯文本视图里只是噪音）。
/// 配对失败时按普通字符处理，保证终止。
fn drop_images(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(bang) = rest.find("![") {
        out.push_str(&rest[..bang]);
        let inner = &rest[bang + 2..];
        let Some(close) = inner.find(']') else {
            out.push('!');
            rest = &rest[bang + 1..];
            continue;
        };
        if !inner[close + 1..].starts_with('(') {
            out.push('!');
            rest = &rest[bang + 1..];
            continue;
        }
        let Some(end) = inner[close + 2..].find(')') else {
            out.push('!');
            rest = &rest[bang + 1..];
            continue;
        };
        rest = &inner[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// `[文本](链接)` → 文本（文本为空或与链接相同时保留链接本身）。
/// 配对失败时按普通字符处理，保证终止。
fn rewrite_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let inner = &rest[open + 1..];
        let Some(close) = inner.find("](") else {
            out.push('[');
            rest = inner;
            continue;
        };
        let Some(end) = inner[close + 2..].find(')') else {
            out.push('[');
            rest = inner;
            continue;
        };
        let text = inner[..close].trim();
        let url = &inner[close + 2..close + 2 + end];
        if text.is_empty() || text == url {
            out.push_str(url);
        } else {
            out.push_str(text);
        }
        rest = &inner[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// 单行转换：标题 / 列表 / 分隔线 / 行内标记。
fn convert_line(line: &str) -> Option<String> {
    let stripped = strip_html(line);
    let trimmed = stripped.trim_end();
    let had_html = stripped.trim() != line.trim();
    let t = trimmed.trim();
    if t.is_empty() {
        // 整行只有 HTML（徽标图片等）→ 丢弃；原本的空行保留为空行
        return if had_html { None } else { Some(String::new()) };
    }

    // 分隔线：整行只由 - * _ ~ 组成且长度 ≥ 3
    if t.len() >= 3 && t.chars().all(|c| matches!(c, '-' | '*' | '_' | '~')) {
        return None;
    }

    // 标题 `#`~`######`：剥标记；三级以下用「【】」突出小节；changelog 版本头剥方括号
    let hashes = t.chars().take_while(|&c| c == '#').count();
    let line = if hashes > 0 && hashes <= 6 {
        let content = t[hashes..].trim();
        let content = content
            .strip_prefix('[')
            .and_then(|c| c.find("] ").map(|p| format!("{}{}", &c[..p], &c[p + 1..])))
            .map(|s| s.to_string())
            .unwrap_or_else(|| content.to_string());
        if hashes >= 3 {
            format!("【{content}】")
        } else {
            content
        }
    } else {
        t.to_string()
    };

    let line = rewrite_links(&drop_images(&line));
    let line = remove_pair(&line, "**");
    let line = line.replace('`', "");

    // 列表项 → 圆点
    let line = if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        format!("• {rest}")
    } else {
        line
    };

    // 引用前缀
    let line = line
        .strip_prefix("> ")
        .map(|s| s.to_string())
        .unwrap_or(line);

    if line.trim().is_empty() {
        None
    } else {
        Some(line)
    }
}

/// 把 Release 正文转成干净的纯文本。
pub fn plain(body: &str) -> String {
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    for line in normalized.lines() {
        match convert_line(line) {
            Some(l) => {
                // 折叠连续空行
                if l.is_empty() && out.last().is_some_and(|p| p.is_empty()) {
                    continue;
                }
                out.push(l);
            }
            None => continue,
        }
    }
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_typical_release_body() {
        let body = "## [0.6.2] - 2026-10-28\n\
                    \n\
                    ### 修复\n\
                    \n\
                    - **「立即重启」失败**：运行中的进程读 `/proc/self/exe` 会得到 `路径 (deleted)`。\n\
                    - 主按钮 hover 文字撞色\n\
                    \n\
                    ---\n\
                    \n\
                    <a href=\"https://slint.dev\"><img src=\"https://example.com/logo.png\" alt=\"Made with Slint\"></a>\n\
                    \n\
                    本项目使用 [Slint](https://slint.dev) 1.18.1，按许可使用。\n\
                    \n\
                    \n\
                    **Full Changelog**: https://github.com/x/y/compare/0.6.1...0.6.2\n";
        let out = plain(body);
        assert!(!out.contains('#'), "标题标记残留: {out}");
        assert!(!out.contains("**"), "加粗标记残留: {out}");
        assert!(!out.contains('`'), "反引号残留: {out}");
        assert!(!out.contains('<'), "HTML 标签残留: {out}");
        assert!(!out.contains("example.com"), "图片行残留: {out}");
        assert!(out.contains("【修复】"), "小节标题: {out}");
        assert!(out.contains("• 「立即重启」失败"), "列表圆点: {out}");
        assert!(out.contains("/proc/self/exe"), "行内代码内容: {out}");
        assert!(out.contains("本项目使用 Slint 1.18.1"), "链接取文本: {out}");
        assert!(
            out.contains("https://github.com/x/y/compare/0.6.1...0.6.2"),
            "裸链接保留: {out}"
        );
        assert!(!out.contains("\n\n\n"), "连续空行未折叠: {out}");
    }

    #[test]
    fn keeps_plain_text_untouched() {
        assert_eq!(plain("hello\nworld"), "hello\nworld");
        assert_eq!(plain(""), "");
        assert_eq!(plain("\n\n"), "");
    }

    #[test]
    fn empty_link_text_keeps_url() {
        assert_eq!(plain("[](https://a.b)"), "https://a.b");
        assert_eq!(plain("[https://a.b](https://a.b)"), "https://a.b");
    }

    #[test]
    fn markdown_image_is_dropped() {
        assert_eq!(plain("前文 ![alt](https://x/y.png) 后文"), "前文  后文");
    }
}
