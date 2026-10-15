//! Docker Panel 程序入口：创建 eframe 原生窗口并启动 egui 应用

mod app;
mod docker;
mod models;
mod utils;
mod widgets;

fn main() -> eframe::Result {
    // 原生窗口选项：默认 1000x700，支持缩放（用户可拖拽改变窗口大小）
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([820.0, 520.0])
            .with_title("Docker Panel"),
        ..Default::default()
    };

    eframe::run_native(
        "Docker Panel",
        options,
        Box::new(|cc| Ok(Box::new(app::DvApp::new(cc)))),
    )
}
