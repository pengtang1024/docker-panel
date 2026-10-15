//! 纯 UI 基础组件与视觉常量：图标码位、按钮、表头、选择框、图表

//! 全部为无状态自由函数，供 app 各渲染模块复用。

use egui::Color32;

/// 最深背景色（顶栏/日志底色），与主题 `dark_bg` 保持一致
pub const DEEP_BG: Color32 = Color32::from_rgb(13, 17, 23);

/// 中性按钮前景色（无语义色的次要操作，如刷新/取消）
pub const NEUTRAL: Color32 = Color32::from_rgb(201, 207, 214);

/// Material Icons 码位（从连字表反查 + 渲染目视验证过的值）
pub mod ic;

mod chart;
mod control;
mod table;

pub use chart::*;
pub use control::*;
pub use table::*;
