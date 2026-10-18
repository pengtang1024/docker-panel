//! 系统主题检测：自动模式下判断桌面环境是否为暗色偏好
//!
//! 检测优先级：
//! 1. `gsettings org.gnome.desktop.interface color-scheme`（Freedesktop 标准）
//! 2. `gsettings org.gnome.desktop.interface gtk-theme` 名称是否含 "dark"
//! 3. `~/.config/gtk-{3,4}.0/settings.ini` 中的 gtk-theme-name 是否含 "dark"
//!
//! 全部失败则回退为浅色（保守默认）。

use std::path::PathBuf;
use std::process::Command;

/// 检测系统是否偏好暗色主题
pub fn detect_system_dark() -> bool {
    // 方式 1：gsettings color-scheme
    // 值可能为 "default" / "prefer-dark" / "prefer-light"
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // gsettings 返回值带单引号：'prefer-dark'
            let clean = s.trim_matches('\'').to_lowercase();
            match clean.as_str() {
                "prefer-dark" => return true,
                "prefer-light" => return false,
                "default" => {} // 继续查 gtk-theme
                _ => {}
            }
        }
    }

    // 方式 2：gsettings gtk-theme 名称含 "dark"
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let clean = s.trim_matches('\'').to_lowercase();
            if clean.contains("dark") {
                return true;
            }
            if !clean.is_empty() {
                return false;
            }
        }
    }

    // 方式 3：GTK settings.ini（GNOME 以外的桌面环境也可能写这里）
    if let Ok(home) = std::env::var("HOME") {
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
                        if name.contains("dark") {
                            return true;
                        }
                        if !name.is_empty() {
                            return false;
                        }
                        break;
                    }
                }
            }
        }
    }

    false
}
