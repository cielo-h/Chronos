#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod fonts;
mod ipc;
mod logger;
mod ui;
mod video;
mod window;

use anyhow::Result;
use config::AppConfig;
use single_instance::SingleInstance;
use ui::app::App;

pub const APP_NAME: &str = "Chronos";

fn main() -> Result<()> {
    let initial_file = std::env::args().nth(1);

    let instance = SingleInstance::new(APP_NAME)?;
    if !instance.is_single() {
        if let Some(path) = initial_file {
            ipc::send_path(path);
        }
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    ipc::start_server(tx);

    logger::init_logger()?;
    log::info!("Application initializing...");

    ffmpeg_next::init().map_err(|e| anyhow::anyhow!("FFmpeg init failed: {e}"))?;

    let (config, config_loaded) = AppConfig::load();
    let viewport = window::build_viewport(&config, config_loaded)?;

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
            fonts::setup(&cc.egui_ctx);
            Ok(Box::new(App::new(cc, config, initial_file, rx)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    log::info!("Application closed");
    Ok(())
}
