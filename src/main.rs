#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod logger;

mod ui;
mod video;

use anyhow::Result;
use config::AppConfig;
use eframe::egui;
use single_instance::SingleInstance;
use ui::app::App;

const APP_NAME: &str = "Chronos";

fn main() -> Result<()> {
    let instance = SingleInstance::new(APP_NAME);
    if !instance?.is_single() {
        return Ok(());
    }

    let _ = logger::init_logger();
    log::info!("Application initializing...");

    // Initialize FFmpeg
    if let Err(e) = ffmpeg_next::init() {
        log::error!("FFmpeg init failed: {:?}", e);
        return Err(anyhow::anyhow!("FFmpeg init failed: {}", e));
    }

    let (config, config_loaded) = AppConfig::load();

    let icon = {
        let bytes = include_bytes!("../icon.png");
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = img.dimensions();
        egui::IconData {
            rgba: img.into_raw(),
            width,
            height,
        }
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_icon(icon)
        .with_title(APP_NAME)
        .with_min_inner_size([1200.0, 900.0])
        .with_inner_size([config.window_width as f32, config.window_height as f32])
        .with_maximized(config.is_maximized);

    if config_loaded {
        viewport = viewport.with_position([config.window_x as f32, config.window_y as f32]);
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        centered: !config_loaded,
        viewport,
        ..Default::default()
    };
    
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new(cc, config)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    log::info!("Application closed");
    Ok(())
}

fn setup_custom_fonts(ctx: &egui::Context) {
    const FONT_NAME: &str = "NotoSans";
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        FONT_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../lib/fonts/NotoSansJP-Regular.ttf"
        ))),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, FONT_NAME.to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, FONT_NAME.to_owned());

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    ctx.set_fonts(fonts);
}
