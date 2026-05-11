use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HotkeyConfig {
    #[serde(default = "hotkey_play_pause")]
    pub play_pause: egui::Key,
    #[serde(default = "hotkey_seek_backward")]
    pub seek_backward: egui::Key,
    #[serde(default = "hotkey_seek_forward")]
    pub seek_forward: egui::Key,
    #[serde(default = "hotkey_seek_start")]
    pub seek_start: egui::Key,
    #[serde(default = "hotkey_seek_end")]
    pub seek_end: egui::Key,
    #[serde(default = "hotkey_frame_step")]
    pub frame_step: egui::Key,
    #[serde(default = "hotkey_frame_back_step")]
    pub frame_back_step: egui::Key,
    #[serde(default = "hotkey_seek_marker")]
    pub seek_marker: egui::Key,
    #[serde(default = "hotkey_mute")]
    pub mute: egui::Key,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            play_pause: hotkey_play_pause(),
            seek_backward: hotkey_seek_backward(),
            seek_forward: hotkey_seek_forward(),
            seek_start: hotkey_seek_start(),
            seek_end: hotkey_seek_end(),
            frame_step: hotkey_frame_step(),
            frame_back_step: hotkey_frame_back_step(),
            seek_marker: hotkey_seek_marker(),
            mute: hotkey_mute(),
        }
    }
}

fn hotkey_play_pause() -> egui::Key {
    egui::Key::Space
}
fn hotkey_seek_backward() -> egui::Key {
    egui::Key::ArrowLeft
}
fn hotkey_seek_forward() -> egui::Key {
    egui::Key::ArrowRight
}
fn hotkey_seek_start() -> egui::Key {
    egui::Key::Home
}
fn hotkey_seek_end() -> egui::Key {
    egui::Key::End
}
fn hotkey_frame_step() -> egui::Key {
    egui::Key::Period
}
fn hotkey_frame_back_step() -> egui::Key {
    egui::Key::Comma
}
fn hotkey_seek_marker() -> egui::Key {
    egui::Key::Z
}
fn hotkey_mute() -> egui::Key {
    egui::Key::M
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "window_x")]
    pub window_x: i32,
    #[serde(default = "window_y")]
    pub window_y: i32,
    #[serde(default = "window_width")]
    pub window_width: i32,
    #[serde(default = "window_height")]
    pub window_height: i32,
    #[serde(default)]
    pub is_maximized: bool,
    #[serde(default = "volume")]
    pub volume: u32,
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_x: window_x(),
            window_y: window_y(),
            window_width: window_width(),
            window_height: window_height(),
            is_maximized: false,
            volume: volume(),
            hotkeys: HotkeyConfig::default(),
        }
    }
}

fn window_x() -> i32 {
    100
}
fn window_y() -> i32 {
    100
}
fn window_width() -> i32 {
    1300
}
fn window_height() -> i32 {
    900
}
fn volume() -> u32 {
    40
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("Failed to get config directory")?
        .join(crate::APP_NAME);

    fs::create_dir_all(&dir).context("Failed to create config directory")?;

    Ok(dir.join(CONFIG_FILE))
}

impl AppConfig {
    pub fn load() -> (Self, bool) {
        match Self::load_inner() {
            Ok(config) => (config, true),
            Err(e) => {
                log::warn!("Failed to load config, using default values: {}", e);
                (Self::default(), false)
            }
        }
    }

    fn load_inner() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Err(anyhow::anyhow!("config not found"));
        }
        let data = fs::read_to_string(&path).context("Failed to read config.json")?;
        let config = serde_json::from_str(&data).context("Failed to parse config.json")?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        let data = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path, data).context("Failed to write config.json")?;
        Ok(())
    }
}
