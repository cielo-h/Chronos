use anyhow::Result;
use eframe::egui;

pub fn build_viewport(
    config: &crate::config::AppConfig,
    config_loaded: bool,
) -> Result<egui::ViewportBuilder> {
    let icon = load_icon()?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_icon(icon)
        .with_title(crate::APP_NAME)
        .with_min_inner_size([1300.0, 900.0])
        .with_inner_size([config.window_width as f32, config.window_height as f32])
        .with_maximized(config.is_maximized);

    if config_loaded {
        viewport = viewport.with_position([config.window_x as f32, config.window_y as f32]);
    }

    Ok(viewport)
}

fn load_icon() -> Result<egui::IconData> {
    let bytes = include_bytes!("../icon.png");
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = img.dimensions();
    Ok(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}
