//! 系统字体扫描与注入：CJK 中文字体 + Material 图标字体

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// egui 默认字体不含中文，启动时扫描系统字体目录并注入 CJK 字体
pub fn load_cjk_fonts(ctx: &Context) {
    // 候选字体目录（按优先级）
    let mut dirs: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/usr/share/fonts"),
        std::path::PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(std::path::PathBuf::from(&home).join(".local/share/fonts"));
        dirs.push(std::path::PathBuf::from(home).join(".fonts"));
    }

    // 候选字体文件关键词（按优先级：思源黑体 > Noto CJK > 文泉驿 > Droid 备用）
    const KEYWORDS: [&str; 6] = [
        "sourcehansans",
        "notosanscjk",
        "notoserifcjk",
        "wqy",
        "droidsansfallback",
        "uming",
    ];

    // 按关键词优先级挑选出的最佳字体文件
    let mut best: Option<(usize, u8, std::path::PathBuf)> = None;
    for dir in &dirs {
        collect_fonts(dir, &mut |path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            for (i, kw) in KEYWORDS.iter().enumerate() {
                if name.contains(kw) {
                    // 同族内优先 Regular 字重，避免选到 Black/Bold 等过粗字重
                    let weight = font_weight_rank(&name);
                    let better = best
                        .as_ref()
                        .map(|(bi, bw, _)| (i, weight) < (*bi, *bw))
                        .unwrap_or(true);
                    if better {
                        best = Some((i, weight, path.to_path_buf()));
                    }
                    return;
                }
            }
        });
    }

    // Material 图标字体（PUA 码位 U+E000-U+F8FF 只有它有字形，放家族尾部即成为 fallback，
    // 不影响拉丁/中文的正常渲染）
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "icons".to_owned(),
        FontData::from_static(include_bytes!("../../assets/MaterialIcons-Regular.ttf")).into(),
    );

    // 中文字体缺失时降级运行（图标仍可用）
    let mut cjk_label: Option<String> = None;
    if let Some((_, _, font_path)) = best {
        match std::fs::read(&font_path) {
            Ok(bytes) => {
                fonts
                    .font_data
                    .insert("cjk".to_owned(), FontData::from_owned(bytes).into());
                cjk_label = Some(font_path.display().to_string());
            }
            Err(_) => log_warn(format!("读取字体失败: {}", font_path.display())),
        }
    }
    if cjk_label.is_none() {
        log_warn("未找到系统中文字体，中文可能显示为方块");
    }

    // 同族字体追加到列表末尾，避免覆盖 egui 默认字体
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            if cjk_label.is_some() {
                list.push("cjk".to_owned());
            }
            list.push("icons".to_owned());
        }
    }
    ctx.set_fonts(fonts);
    if let Some(label) = cjk_label {
        log_info(format!("已加载中文字体: {label}"));
    }
}

/// 字重排序（越小越优先）：Regular 最佳，Black/Heavy 等过粗字重最后
fn font_weight_rank(lower_name: &str) -> u8 {
    if lower_name.contains("regular") {
        0
    } else if lower_name.contains("medium") || lower_name.contains("book") {
        1
    } else if lower_name.contains("light") || lower_name.contains("demi") {
        2
    } else if lower_name.contains("bold")
        || lower_name.contains("black")
        || lower_name.contains("heavy")
    {
        4
    } else {
        // 未标注字重的按中性处理
        3
    }
}

/// 递归收集目录下的字体文件（.ttf/.otf/.ttc）
fn collect_fonts(dir: &std::path::Path, out: &mut impl FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fonts(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if ext == "ttf" || ext == "otf" || ext == "ttc" {
                out(&path);
            }
        }
    }
}

/// 简单的标准输出日志（GUI 程序无终端交互需求，直接打印）
fn log_info(msg: impl AsRef<str>) {
    println!("[dvkit] {}", msg.as_ref());
}

fn log_warn(msg: impl AsRef<str>) {
    eprintln!("[dvkit][警告] {}", msg.as_ref());
}
