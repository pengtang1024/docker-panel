//! 系统主题检测：自动模式下判断桌面环境是否为暗色偏好
//!
//! 检测优先级（成功即返回，不再尝试下一级）：
//! 1. `gsettings org.gnome.desktop.interface color-scheme`（Freedesktop 标准，明确值）
//! 2. `gsettings org.gnome.desktop.interface gtk-theme` 名称是否含 "dark"
//! 3. `~/.config/gtk-{3,4}.0/settings.ini` 中的 gtk-theme-name 是否含 "dark"
//!
//! 返回 `Option<bool>`：
//! - `Some(true)` = 明确检测到暗色偏好
//! - `Some(false)` = 明确检测到浅色偏好
//! - `None` = 所有检测方式都失败（无 gsettings、无 GTK 配置），调用方自行决定默认值

use std::path::PathBuf;
use std::process::Command;

/// 检测系统是否偏好暗色主题。`None` 表示检测全失败，不代表浅色。
pub fn detect_system_dark() -> Option<bool> {
    // 方式 1：gsettings color-scheme（值为 "prefer-dark" / "prefer-light" 时最可靠）
    if let Some(scheme) = run_gsettings("get", &["org.gnome.desktop.interface", "color-scheme"]) {
        let clean = scheme.trim_matches('\'').to_lowercase();
        match clean.as_str() {
            "prefer-dark" => return Some(true),
            "prefer-light" => return Some(false),
            "default" => {} // 继续查 gtk-theme（"default" 本身不带偏好）
            _ => {}
        }
    }

    // 方式 2：gsettings gtk-theme 名称含 "dark" = 暗色；非空且不含 = 浅色
    if let Some(theme) = run_gsettings("get", &["org.gnome.desktop.interface", "gtk-theme"]) {
        let clean = theme.trim_matches('\'').to_lowercase();
        if !clean.is_empty() {
            return Some(clean.contains("dark"));
        }
    }

    // 方式 3：GTK settings.ini（GNOME 以外的桌面环境也可能写这里）
    if let Some(home) = std::env::var_os("HOME") {
        for ver in ["4.0", "3.0"] {
            let mut p = PathBuf::new();
            p.push(&home);
            p.push(".config");
            p.push(format!("gtk-{ver}"));
            p.push("settings.ini");
            if let Ok(content) = std::fs::read_to_string(&p) {
                for line in content.lines() {
                    if let Some(val) = line.strip_prefix("gtk-theme-name=") {
                        let name = val.trim().trim_matches('"').to_lowercase();
                        if !name.is_empty() {
                            return Some(name.contains("dark"));
                        }
                        break;
                    }
                }
            }
        }
    }

    None
}

/// 执行 gsettings 命令，返回 stdout 裁剪后的字符串；任何失败返回 None
fn run_gsettings(args: &str, rest: &[&str]) -> Option<String> {
    let output = Command::new("gsettings")
        .arg(args)
        .args(rest)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
