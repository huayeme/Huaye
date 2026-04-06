#![windows_subsystem = "windows"]

mod app;
mod components;
mod core;
mod modules;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    // 创建事件通道 (在日志初始化之前，以便 panic hook 能发送 FatalError)
    let (event_tx, event_rx) = flume::bounded(50_000);

    // 初始化日志与全局遥测 (GLOBAL_EVENT_TX 在此初始化)
    // _log_guard 必须存活到 main 结束，否则非阻塞日志写入器会被关闭
    let _log_guard = core::logger::init(event_tx);

    // 1. 初始化 Tokio 运行时 (按架构要求)
    let _rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");
    let _enter = _rt.enter();

    // 加载图标
    let icon_data =
        if let Ok(image) = image::load_from_memory(include_bytes!("static/app_icon.png")) {
            let image = image.to_rgba8();
            let (width, height) = image.dimensions();
            Some(std::sync::Arc::new(egui::IconData {
                rgba: image.into_raw(),
                width,
                height,
            }))
        } else {
            None
        };

    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(false)
        .with_inner_size([
            core::theme::MIN_VISUAL_WIDTH + core::theme::SHADOW_MARGIN * 2.0,
            core::theme::MIN_VISUAL_HEIGHT + core::theme::SHADOW_MARGIN * 2.0,
        ])
        .with_min_inner_size([
            core::theme::MIN_VISUAL_WIDTH + core::theme::SHADOW_MARGIN * 2.0,
            core::theme::MIN_VISUAL_HEIGHT + core::theme::SHADOW_MARGIN * 2.0,
        ]);

    if let Some(icon) = icon_data {
        viewport = viewport.with_icon(icon);
    }

    // 2. 初始化 eframe 应用
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "花也 - Foundation (Modular)",
        options,
        Box::new(|cc| {
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(app::MyApp::new(event_rx)))
        }),
    )
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "harmony".to_owned(),
        egui::FontData::from_static(include_bytes!("static/HarmonyOS.ttf")).into(),
    );

    if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        vec.insert(0, "harmony".to_owned());
    }
    if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        vec.insert(0, "harmony".to_owned());
    }

    ctx.set_fonts(fonts);
}
