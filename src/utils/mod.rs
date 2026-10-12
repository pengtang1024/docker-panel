//! 通用工具函数：字节数/时间格式化、状态着色、中文字体加载、全局主题

mod fonts;
pub use fonts::load_cjk_fonts;

use egui::{Color32, Context, CornerRadius, Margin, Stroke, TextStyle, Visuals};

// ---------- 主题色板（GitHub Dark 风格） ----------
/// 主强调色：蓝
pub const ACCENT: Color32 = Color32::from_rgb(31, 111, 235);
/// 亮蓝：链接/强调文字
pub const BLUE: Color32 = Color32::from_rgb(88, 166, 255);
/// 绿：运行中/启动
pub const GREEN: Color32 = Color32::from_rgb(63, 185, 80);
/// 红：停止/删除/错误
pub const RED: Color32 = Color32::from_rgb(248, 81, 73);
/// 琥珀：重启/警告
pub const AMBER: Color32 = Color32::from_rgb(210, 153, 34);
/// 青：监控
pub const CYAN: Color32 = Color32::from_rgb(57, 197, 207);
/// 面板背景
pub const PANEL_BG: Color32 = Color32::from_rgb(22, 27, 34);
/// 表头/输入框背景
pub const SURFACE_BG: Color32 = Color32::from_rgb(33, 38, 45);
/// 次要文字
pub const TEXT_DIM: Color32 = Color32::from_rgb(139, 148, 158);
/// 分隔线
pub const DIVIDER: Color32 = Color32::from_rgb(48, 54, 61);

/// 全局主题：深蓝暗色调 + 细节打磨（比 egui 默认暗色更精致）
pub fn apply_theme(ctx: &Context) {
    // 最深背景（顶栏/日志底色）
    const DEEP_BG: Color32 = Color32::from_rgb(13, 17, 23);

    let mut v = Visuals::dark();

    // 背景层次：面板 → 弹窗 → 输入框/表头
    v.panel_fill = PANEL_BG;
    v.window_fill = PANEL_BG;
    v.window_stroke = Stroke::new(1.0, DIVIDER);
    v.extreme_bg_color = SURFACE_BG;
    v.faint_bg_color = Color32::from_rgb(30, 35, 43); // 斑马纹：与面板底拉开层次
    v.code_bg_color = DEEP_BG;

    // 圆角
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);

    // 弹窗柔和投影，营造悬浮层级感
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };

    // 强调与语义色
    v.selection.bg_fill = ACCENT;
    // 输入框获焦边框用强调蓝，替代生硬的白框
    v.selection.stroke = Stroke::new(1.0, BLUE);
    v.hyperlink_color = BLUE;
    v.warn_fg_color = AMBER;
    v.error_fg_color = RED;
    v.weak_text_alpha = 0.75;

    // 控件三态
    let a = &mut v.widgets.active;
    a.bg_fill = ACCENT;
    a.weak_bg_fill = ACCENT;
    a.bg_stroke = Stroke::new(1.0, BLUE);
    a.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    a.corner_radius = CornerRadius::same(6);
    let h = &mut v.widgets.hovered;
    h.bg_fill = Color32::from_rgb(55, 61, 71);
    h.weak_bg_fill = Color32::from_rgb(55, 61, 71);
    // 悬停描边压暗，避免“描边框”的老式观感
    h.bg_stroke = Stroke::new(1.0, Color32::from_rgb(76, 83, 94));
    h.fg_stroke = Stroke::new(1.0, Color32::from_rgb(240, 243, 248));
    h.corner_radius = CornerRadius::same(6);
    let i = &mut v.widgets.inactive;
    i.bg_fill = SURFACE_BG;
    i.weak_bg_fill = SURFACE_BG;
    i.bg_stroke = Stroke::new(1.0, DIVIDER);
    i.fg_stroke = Stroke::new(1.0, Color32::from_rgb(201, 207, 214));
    i.corner_radius = CornerRadius::same(6);
    let n = &mut v.widgets.noninteractive;
    n.bg_fill = PANEL_BG;
    n.weak_bg_fill = PANEL_BG;
    n.bg_stroke = Stroke::new(1.0, DIVIDER);
    n.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    n.corner_radius = CornerRadius::same(6);

    ctx.set_visuals(v);

    // 间距与字号
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 3.0);
        style.spacing.window_margin = Margin::same(14);
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.indent = 16.0;
        // 现代悬浮滚动条：平时隐藏，悬停时浮现
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        style.text_styles.insert(
            TextStyle::Heading,
            egui::FontId::new(17.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Body,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            egui::FontId::new(12.5, egui::FontFamily::Monospace),
        );
    });
}

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
        "running" => ("运行中", GREEN),
        "exited" => ("已停止", TEXT_DIM),
        "created" => ("已创建", BLUE),
        "paused" => ("已暂停", AMBER),
        "restarting" => ("重启中", AMBER),
        "removing" => ("删除中", AMBER),
        "dead" => ("失效", RED),
        "created_but_unmanaged" => ("未托管", TEXT_DIM),
        _ => ("未知", TEXT_DIM),
    }
}
