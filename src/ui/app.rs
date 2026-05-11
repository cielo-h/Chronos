use crate::config::AppConfig;
use crate::ui::log_window::LogWindow;
use crate::ui::seekbar::SeekbarWidget;
use crate::ui::toast::*;
use crate::video::mpv_manager::MpvManager;
use crate::video::mpv_texture_renderer::MpvTextureRenderer;
use crate::video::thumbnail::ThumbnailManager;
use arboard::Clipboard;
use eframe::{Frame, egui};
use egui::Ui;
use egui_glow::CallbackFn;
use egui_phosphor::regular;
use glow::Context;
use rfd::FileDialog;
use std::sync::{Arc, Mutex};

pub struct App {
    mpv: MpvManager,
    gl: Arc<Context>,
    gl_renderer: Arc<Mutex<MpvTextureRenderer>>,
    config: AppConfig,
    log_window: LogWindow,
    thumbnail_manager: Option<ThumbnailManager>,
    clipboard: Option<Clipboard>,
    toast: Option<Toast>,
    drag_pos: Option<f64>,
    last_seek_time: f64,
    pre_maximize_rect: Option<egui::Rect>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: AppConfig,
        initial_file: Option<String>,
    ) -> Self {
        let get_proc = cc
            .get_proc_address
            .as_ref()
            .expect("eframe must be initialised with a GL context");

        Self::set_initial_visual(cc);

        let gl = cc.gl.as_ref().expect("eframe GL context unavailable");
        let gl_renderer = Arc::new(Mutex::new(MpvTextureRenderer::new(gl)));

        let mut app = Self {
            mpv: MpvManager::new(&**get_proc, &config).expect(""),
            gl: Arc::clone(gl),
            gl_renderer,
            config,
            log_window: LogWindow::new(),
            thumbnail_manager: None,
            clipboard: Clipboard::new().ok(),
            toast: None,
            drag_pos: None,
            last_seek_time: 0.0,
            pre_maximize_rect: None,
        };

        if let Some(initial_file) = initial_file {
            app.open(initial_file);
        }

        app
    }

    // ── Private ──────────────────────────────────────────────────────────

    fn set_initial_visual(cc: &eframe::CreationContext<'_>) {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::WHITE);
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 60));
        visuals.panel_fill = egui::Color32::from_rgb(30, 30, 30);
        cc.egui_ctx.set_visuals(visuals);
    }

    fn show_toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast::new(text, kind));
    }

    fn open_file(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter(
                "Video Files",
                &[
                    "mp4", "mkv", "ts", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpeg", "mpg",
                    "m2ts", "mxf",
                ],
            )
            .pick_file()
        {
            let path_str = path.display().to_string();
            log::info!("Opening file: {}", path_str);

            self.open(path_str);
        }
    }

    fn open(&mut self, path: String) {
        self.thumbnail_manager = Some(ThumbnailManager::new(path.clone()));

        self.drag_pos = None;
        self.last_seek_time = 0.0;

        self.mpv.load_file(&path);
    }

    fn capture_frame(&self) -> Option<(Vec<u8>, u32, u32)> {
        let tex_id = self.mpv.video_texture_id();

        if tex_id == 0 {
            return None;
        }

        let tex_size = self.mpv.video_size();

        self.gl_renderer
            .lock()
            .ok()
            .and_then(|r| r.capture_texture(&self.gl, tex_id, tex_size.x as u32, tex_size.y as u32))
    }

    fn save_frame_as_image(&mut self) {
        let Some((rgb_data, width, height)) = self.capture_frame() else {
            self.show_toast("Failed to capture frame", ToastKind::Error);
            return;
        };

        let was_playing = self.mpv.is_playing;

        if was_playing {
            self.mpv.toggle_pause();
        }

        let Some(path) = FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("capture.png")
            .save_file()
        else {
            return;
        };

        let img =
            image::RgbImage::from_raw(width, height, rgb_data).expect("Failed to create image");

        if let Err(e) = img.save(&path) {
            let msg = "Save failed";
            log::error!("{}: {}", msg, e);
            self.show_toast(msg, ToastKind::Error);
        } else {
            let msg = "Saved successfully";
            log::info!("{}: {:?}", msg, path);
            self.show_toast(msg, ToastKind::Success);
        }

        if was_playing {
            self.mpv.toggle_pause();
        }
    }

    fn copy_frame_to_clipboard(&mut self) {
        let Some((rgb_data, width, height)) = self.capture_frame() else {
            self.show_toast("Failed to capture frame", ToastKind::Error);
            return;
        };

        let Some(cb) = &mut self.clipboard else {
            self.show_toast("Failed to get clipboard", ToastKind::Error);
            return;
        };

        let rgba: Vec<u8> = rgb_data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();

        let img = arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Owned(rgba),
        };

        if let Err(e) = cb.set_image(img) {
            let msg = "Failed to copy to clipboard";
            log::error!("{}: {}", msg, e);
            self.show_toast(msg, ToastKind::Error);
        } else {
            let msg = "Copied to clipboard";
            log::info!("{}", msg);
            self.show_toast(msg, ToastKind::Success);
        }
    }
}

const BTN_SIZE: f32 = 16.0;
const ROW_HEIGHT: f32 = 28.0;

impl App {
    fn update(&mut self, _ui: &Ui) {
        self.mpv.update_frame();
    }

    fn update_config(&mut self, ui: &Ui) {
        let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let outer_rect = ui.input(|i| i.viewport().outer_rect);

        if !is_maximized {
            if let Some(pos) = outer_rect {
                self.pre_maximize_rect = Some(pos);
                self.config.window_x = pos.min.x as i32;
                self.config.window_y = pos.min.y as i32;
                self.config.window_width = pos.width() as i32;
                self.config.window_height = pos.height() as i32;
            }
        }

        if !is_maximized && self.config.is_maximized {
            if let Some(rect) = self.pre_maximize_rect {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::OuterPosition(rect.min));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(rect.size()));
            }
        }

        self.config.is_maximized = is_maximized;
    }

    fn handle_shortcuts(&mut self, ui: &Ui) {
        if ui.egui_wants_keyboard_input() {
            return;
        }

        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.mpv.toggle_pause();
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            let shift = if ui.input(|i| i.modifiers.ctrl) {
                -10.0
            } else {
                -5.0
            };
            self.mpv.seek(shift);
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            let shift = if ui.input(|i| i.modifiers.ctrl) {
                10.0
            } else {
                5.0
            };
            self.mpv.seek(shift);
        }

        if ui.input(|i| i.key_pressed(egui::Key::Home)) {
            self.mpv.seek_start();
        }
        if ui.input(|i| i.key_pressed(egui::Key::End)) {
            self.mpv.seek_end();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Period)) {
            self.mpv.frame_step();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Comma)) {
            self.mpv.frame_back_step();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Z)) {
            self.mpv.seek_absolute(self.mpv.marker_time);
        }
    }

    fn handle_drag_and_drop(&mut self, ui: &Ui) {
        ui.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(file) = i.raw.dropped_files.first() {
                    if let Some(path) = &file.path {
                        let path_str = path.display().to_string();
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                        match ext.to_lowercase().as_str() {
                            "mp4" | "mkv" | "ts" | "avi" | "mov" | "wmv" | "flv" | "webm"
                            | "m4v" | "mpeg" | "mpg" | "m2ts" | "mxf" => {
                                log::info!("ファイルをドロップしました: {}", path_str);
                                self.open(path_str);
                            }
                            _ => {
                                let msg = format!("Unsupported file extension: {}", ext);
                                log::warn!("{}", msg);
                                self.show_toast(msg, ToastKind::Error);
                            }
                        }
                    }
                }
            }
        });
    }

    fn ui_menu_bar(&mut self, ui: &Ui) {
        egui::TopBottomPanel::top("menu_bar")
            .frame(egui::Frame::NONE.fill(egui::Color32::DARK_GRAY))
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button(
                        egui::RichText::new("ファイル").color(egui::Color32::WHITE),
                        |ui| {
                            if ui.button("開く").clicked() {
                                self.open_file();
                                ui.close();
                            }

                            if ui.button("ファイルを閉じる").clicked() {
                                self.mpv.close_file();
                                ui.close();
                            }

                            ui.separator();

                            if ui.button("終了").clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close();
                            }
                        },
                    );

                    if ui
                        .button(egui::RichText::new("ログ").color(egui::Color32::WHITE))
                        .clicked()
                    {
                        self.log_window.toggle();
                    }
                });
            });
    }

    fn ui_video_area(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                let tex_id = self.mpv.video_texture_id();

                let available_rect = ui.available_rect_before_wrap();

                let tex_size = self.mpv.video_size();
                let scale =
                    (available_rect.width() / tex_size.x).min(available_rect.height() / tex_size.y);
                let display_size = tex_size * scale;

                let rect = egui::Rect::from_center_size(available_rect.center(), display_size);
                if tex_id != 0 {
                    let renderer = Arc::clone(&self.gl_renderer);
                    ui.painter().add(egui::PaintCallback {
                        rect,
                        callback: Arc::new(CallbackFn::new(move |info, painter| {
                            if let Ok(r) = renderer.lock() {
                                r.paint(painter.gl(), &info, rect, tex_id);
                            }
                        })),
                    });
                }
            });
    }

    fn ui_seekbar(&mut self, ui: &mut Ui) {
        let mut seek_request = None;
        let mut marker_request = None;
        ui.add(SeekbarWidget::new(
            &self.mpv,
            &mut seek_request,
            &mut marker_request,
            &mut self.thumbnail_manager,
            &mut self.drag_pos,
            &mut self.last_seek_time,
        ));

        if let Some(time) = seek_request {
            self.mpv.seek_absolute(time);
        }

        if let Some(time) = marker_request {
            self.mpv.marker_time = time;
        }
    }

    fn ui_bottom_bar(&mut self, ui: &mut Ui) {
        if ui
            .button(egui::RichText::new(regular::SKIP_BACK).size(BTN_SIZE))
            .on_hover_text("先頭に戻る [HOME]")
            .clicked()
        {
            self.mpv.seek_start();
        }

        if ui
            .button(egui::RichText::new(regular::REWIND).size(BTN_SIZE))
            .on_hover_text(format!("5秒戻る [{}]", regular::ARROW_LEFT))
            .clicked()
        {
            self.mpv.seek(-5.0);
        }

        let play_label = if self.mpv.is_playing {
            regular::PAUSE
        } else {
            regular::PLAY
        };

        if ui
            .button(egui::RichText::new(play_label).size(BTN_SIZE))
            .on_hover_text("再生/一時停止 [SPACE]")
            .clicked()
        {
            self.mpv.toggle_pause();
        }

        if ui
            .button(egui::RichText::new(regular::FAST_FORWARD).size(BTN_SIZE))
            .on_hover_text(format!("5秒進む [{}]", regular::ARROW_RIGHT))
            .clicked()
        {
            self.mpv.seek(5.0);
        }

        if ui
            .button(egui::RichText::new(regular::SKIP_FORWARD).size(BTN_SIZE))
            .on_hover_text("末尾に移動する [END]")
            .clicked()
        {
            self.mpv.seek_end();
        }
    }

    fn ui_volume_control(&mut self, ui: &mut Ui) {
        let mute_label = if self.mpv.is_muted || self.mpv.volume == 0 {
            regular::SPEAKER_SIMPLE_SLASH
        } else {
            regular::SPEAKER_HIGH
        };

        if ui
            .button(egui::RichText::new(mute_label).size(BTN_SIZE))
            .on_hover_text("ミュート")
            .clicked()
        {
            self.mpv.toggle_mute();
        }

        let mut display_vol = if self.mpv.is_muted {
            0u32
        } else {
            self.mpv.volume
        };
        let vol_slider = egui::Slider::new(&mut display_vol, 0..=100)
            .text(egui::RichText::new("%").color(egui::Color32::WHITE));
        let response = ui.add(vol_slider);

        if response.changed() {
            self.mpv.set_mute(false);
            self.mpv.is_muted = false;
            self.mpv.set_volume(display_vol);
        }

        if response.hovered() {
            let scroll_y = ui.input_mut(|i| {
                let mut delta = 0.0;
                i.events.retain(|e| {
                    if let egui::Event::MouseWheel { delta: d, .. } = e {
                        delta = d.y;
                        false
                    } else {
                        true
                    }
                });
                delta
            });
            if scroll_y != 0.0 {
                let step = 5;
                let delta = if scroll_y > 0.0 { step } else { -step };
                let new_vol = (self.mpv.volume as i32 + delta).clamp(0, 120);
                self.mpv.set_volume(new_vol as u32);

                if new_vol == 0 {
                    self.mpv.is_muted = true;
                } else if scroll_y > 0.0 {
                    self.mpv.is_muted = false;
                }
            }
        }
    }

    fn ui_diff_control(&mut self, ui: &mut Ui) {
        ui.set_height(ROW_HEIGHT);

        if ui
            .button(egui::RichText::new(regular::MAP_PIN_SIMPLE).size(BTN_SIZE))
            .on_hover_text("始点を設定")
            .clicked()
        {
            self.mpv.marker_time = self.mpv.current_time;
        }

        let diff = (self.mpv.current_time - self.mpv.marker_time).abs();
        let mins = (diff / 60.0).floor() as u32;
        let secs = diff % 60.0;
        ui.label(
            egui::RichText::new(format!(
                "差分: {:02}分{:04.1}秒 ({:06.1}秒)",
                mins, secs, diff
            ))
                .text_style(egui::TextStyle::Monospace)
                .size(BTN_SIZE)
                .color(egui::Color32::WHITE),
        );
    }

    fn ui_capture_control(&mut self, ui: &mut Ui) {
        if ui
            .button(egui::RichText::new(regular::CAMERA).size(BTN_SIZE))
            .on_hover_text("現在のフレームを保存")
            .clicked()
        {
            self.save_frame_as_image();
        }
        if ui
            .button(egui::RichText::new(regular::CLIPBOARD).size(BTN_SIZE))
            .on_hover_text("現在のフレームをクリップボードにコピー")
            .clicked()
        {
            self.copy_frame_to_clipboard();
        }
    }

    fn ui_convenient_buttons(&mut self, ui: &mut Ui) {
        let button = egui::Button::new(egui::RichText::new("f").size(BTN_SIZE))
            .min_size(egui::vec2(24.0, 24.0));

        let response = ui
            .add(button)
            .on_hover_text("左クリック: 1f戻る / 右クリック: 1f進む");

        if response.clicked() {
            self.mpv.frame_back_step();
        }

        if response.secondary_clicked() {
            self.mpv.frame_step();
        }

        let steps: &[f64] = &[10.0, 15.0, 30.0, 60.0, 90.0, 120.0, 180.0];

        for &step in steps {
            let label = format!("{}", step as i64);
            let hover = format!(
                "左クリック: {}秒戻る / 右クリック: {}秒進む",
                step as i64, step as i64
            );
            let response = ui
                .button(egui::RichText::new(label).size(BTN_SIZE))
                .on_hover_text(hover);

            if response.clicked() {
                self.mpv.seek(-step);
            }
            if response.secondary_clicked() {
                self.mpv.seek(step);
            }
        }
    }

    fn ui_time_view(&mut self, ui: &mut Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let cur = self.mpv.current_time;
            let dur = self.mpv.duration;

            let fmt_time = |t: f64| -> String {
                let h = (t / 3600.0).floor() as u32;
                let m = ((t % 3600.0) / 60.0).floor() as u32;
                let s = t % 60.0;
                if h > 0 {
                    format!("{:02}:{:02}:{:05.2}", h, m, s)
                } else {
                    format!("{:02}:{:05.2}", m, s)
                }
            };

            egui::Frame::NONE
                .fill(egui::Color32::GRAY)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(5, 2))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} / {}", fmt_time(cur), fmt_time(dur)))
                            .size(BTN_SIZE * 1.5)
                            .color(egui::Color32::WHITE),
                    )
                });
        });
    }

    fn ui_control_panel(&mut self, ui: &mut Ui) {
        egui::TopBottomPanel::bottom("control_panel")
            .frame(egui::Frame::NONE.fill(egui::Color32::DARK_GRAY))
            .show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 8,
                        right: 8,
                        top: 0,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        let loaded = self.mpv.is_loaded;

                        ui.add_enabled_ui(loaded, |ui| {
                            self.ui_seekbar(ui);
                        });

                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(loaded, |ui| {
                                self.ui_bottom_bar(ui);
                            });

                            ui.separator();

                            self.ui_volume_control(ui);

                            ui.separator();

                            ui.add_enabled_ui(loaded, |ui| {
                                self.ui_diff_control(ui);
                            });

                            ui.separator();

                            ui.add_enabled_ui(loaded, |ui| {
                                self.ui_capture_control(ui);
                            });

                            ui.separator();

                            ui.add_enabled_ui(loaded, |ui| {
                                self.ui_convenient_buttons(ui);
                            });

                            self.ui_time_view(ui);
                        });
                    });
            });
    }

    fn ui_open_log_window(&mut self, ui: &mut Ui) {
        if self.log_window.open {
            ui.show_viewport_immediate(
                egui::ViewportId::from_hash_of("log_window"),
                egui::ViewportBuilder::default()
                    .with_title("Chronos - Log")
                    .with_min_inner_size([700.0, 400.0]),
                |ctx, class| {
                    assert!(class == egui::ViewportClass::Immediate);

                    egui::CentralPanel::default().show_inside(ctx, |ui| {
                        self.log_window.ui_content(ui);
                    });

                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.log_window.open = false;
                    }
                },
            );
        }
    }

    fn ui_show_toast(&mut self, ui: &mut Ui) {
        if let Some(toast) = &self.toast {
            if toast.start_time.elapsed().as_secs_f32() > 3.0 {
                self.toast = None;
            } else {
                toast.show(ui);
            }
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.update_config(ui);

        self.handle_shortcuts(ui);
        self.handle_drag_and_drop(ui);

        self.update(ui);

        self.ui_menu_bar(ui);

        self.ui_control_panel(ui);

        self.ui_video_area(ui);

        self.ui_show_toast(ui);

        self.ui_open_log_window(ui);

        ui.request_repaint();
    }

    fn on_exit(&mut self, gl: Option<&Context>) {
        if let Some(gl) = gl {
            if let Ok(mut r) = self.gl_renderer.lock() {
                r.destroy(gl);
            }
        }

        self.config.volume = self.mpv.get_volume();
        if let Err(e) = self.config.save() {
            log::error!("Couldn't save config file: {:?}", e);
        }
    }
}