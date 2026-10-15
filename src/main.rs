//! Docker Panel 程序入口：创建 eframe 原生窗口并启动 egui 应用

// pedantic clippy：以下 lint 对本项目（Docker API 返回的整数类型间转换）是过度严苛的
#![allow(
    clippy::cast_precision_loss,        // u64→f64 百分比、i64→f64 排序等，精度可接受
    clippy::cast_sign_loss,             // i64→u64 创建时间（unix 时间戳非负）
    clippy::cast_possible_wrap,         // u64→i64 / usize→i64 同样非负可保证
    clippy::cast_possible_truncation,   // f32→u8 opacity 值在 0.0-1.0
    clippy::too_many_lines,             // 表格渲染函数业务逻辑多但不宜拆分
    clippy::struct_excessive_bools,     // DvApp 状态位多但集中
    clippy::needless_pass_by_value,     // id/name String 参数后续会 clone 或消费
    clippy::needless_pass_by_ref_mut,   // &mut self 是 egui callback 签名
    clippy::clone_on_copy,              // 编译器会优化，可读性优先
    clippy::format_push_string,         // write! 需要 import std::fmt::Write，push_str 更直接
    clippy::unused_self,                // 某些 DvApp 方法 self 是 egui trait 签名要求
    clippy::assigning_clones,           // 两个 String 字段各需同一份值所有权
)]

mod app;
mod docker;
mod models;
mod utils;
mod widgets;

fn load_app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/app-icon.png");
    let img = image::load_from_memory(bytes).expect("failed to decode app-icon.png");
    let rgba = img.to_rgba8();
    egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}

fn main() -> eframe::Result {
    // 原生窗口选项：默认 1000x700，支持缩放（用户可拖拽改变窗口大小）
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([820.0, 520.0])
            .with_decorations(false)
            .with_title("Docker Panel")
            .with_icon(load_app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Docker Panel",
        options,
        Box::new(|cc| Ok(Box::new(app::DvApp::new(cc)))),
    )
}
