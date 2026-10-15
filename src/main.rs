//! dvkit 程序入口：创建 eframe 原生窗口并启动 egui 应用

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
            .with_title("dvkit")
            .with_icon(load_app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "dvkit",
        options,
        Box::new(|cc| Ok(Box::new(app::DvApp::new(cc)))),
    )
}
