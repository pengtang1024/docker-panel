//! 通用工具函数：字节数/时间格式化、状态着色、中文字体加载

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily};

/// 字节数格式化为人类可读文本（KiB/MiB/GiB）
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", value as u64)
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// 秒数格式化为 "X天X小时X分" 简短时长
pub fn duration_text(seconds: i64) -> String {
    if seconds < 0 {
        return "-".into();
    }
    let s = seconds as u64;
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    let mins = (s % 3600) / 60;
    if days > 0 {
        format!("{}天{}小时", days, hours)
    } else if hours > 0 {
        format!("{}小时{}分", hours, mins)
    } else if mins > 0 {
        format!("{}分", mins)
    } else {
        format!("{}秒", s)
    }
}

/// unix 时间戳格式化为相对时间（"3分钟前"、"2天前"）
pub fn format_age(timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let delta = now - timestamp;
    if delta < 0 {
        return "刚刚".into();
    }
    format!("{}前", duration_text(delta))
}

/// 运行状态 → (中文标签, 颜色)
pub fn state_label(state: &str) -> (&'static str, Color32) {
    match state {
        "running" => ("运行中", Color32::from_rgb(46, 204, 113)),
        "exited" => ("已停止", Color32::from_rgb(149, 165, 166)),
        "created" => ("已创建", Color32::from_rgb(52, 152, 219)),
        "paused" => ("已暂停", Color32::from_rgb(241, 196, 15)),
        "restarting" => ("重启中", Color32::from_rgb(230, 126, 34)),
        "removing" => ("删除中", Color32::from_rgb(230, 126, 34)),
        "dead" => ("失效", Color32::from_rgb(231, 76, 60)),
        "created_but_unmanaged" => ("未托管", Color32::from_rgb(149, 165, 166)),
        _ => ("未知", Color32::from_rgb(149, 165, 166)),
    }
}

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
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    for dir in &dirs {
        collect_fonts(dir, &mut |path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            for (i, kw) in KEYWORDS.iter().enumerate() {
                if name.contains(kw) {
                    let better = best.as_ref().map(|(bi, _)| i < *bi).unwrap_or(true);
                    if better {
                        best = Some((i, path.to_path_buf()));
                    }
                    return;
                }
            }
        });
    }

    let Some((_, font_path)) = best else {
        log_warn("未找到系统中文字体，中文可能显示为方块");
        return;
    };

    let Ok(bytes) = std::fs::read(&font_path) else {
        log_warn(format!("读取字体失败: {}", font_path.display()));
        return;
    };

    let mut fonts = FontDefinitions::default();
    // 同族字体追加到列表末尾，避免覆盖 egui 默认字体
    fonts
        .font_data
        .insert("cjk".to_owned(), FontData::from_owned(bytes).into());
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.push("cjk".to_owned());
        }
    }
    ctx.set_fonts(fonts);
    log_info(format!("已加载中文字体: {}", font_path.display()));
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
