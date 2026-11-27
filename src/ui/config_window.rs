use crate::config::HotkeyConfig;
use egui_phosphor::regular;

const ACTIONS: &[(&str, &str)] = &[
    ("play_pause", "Play / Pause"),
    ("seek_backward", "Seek Backward"),
    ("seek_forward", "Seek Forward"),
    ("seek_start", "Seek Start"),
    ("seek_end", "Seek End"),
    ("frame_step", "Frame Step"),
    ("frame_back_step", "Frame Back Step"),
    ("seek_marker", "Seek to Marker"),
    ("mute", "Mute"),
    ("zoom_quarter", "Window 1/4 (Alt+)"),
    ("zoom_half", "Window 1/2 (Alt+)"),
    ("zoom_original", "Window 1:1 (Alt+)"),
    ("zoom_double", "Window 2:1 (Alt+)"),
];

pub fn key_label(key: egui::Key) -> String {
    match key {
        egui::Key::ArrowLeft => regular::ARROW_LEFT.to_string(),
        egui::Key::ArrowRight => regular::ARROW_RIGHT.to_string(),
        egui::Key::ArrowUp => regular::ARROW_UP.to_string(),
        egui::Key::ArrowDown => regular::ARROW_DOWN.to_string(),
        _ => key.symbol_or_name().to_string(),
    }
}

#[derive(Default)]
pub struct ConfigWindow {
    pub open: bool,
    pub capturing: Option<String>,
    conflict: Option<ConflictState>,
}

struct ConflictState {
    action: String,
    key: egui::Key,
    conflicting: String,
}

fn get_key(hotkeys: &HotkeyConfig, action: &str) -> egui::Key {
    match action {
        "play_pause" => hotkeys.play_pause,
        "seek_backward" => hotkeys.seek_backward,
        "seek_forward" => hotkeys.seek_forward,
        "seek_start" => hotkeys.seek_start,
        "seek_end" => hotkeys.seek_end,
        "frame_step" => hotkeys.frame_step,
        "frame_back_step" => hotkeys.frame_back_step,
        "seek_marker" => hotkeys.seek_marker,
        "mute" => hotkeys.mute,
        "zoom_quarter" => hotkeys.zoom_quarter,
        "zoom_half" => hotkeys.zoom_half,
        "zoom_original" => hotkeys.zoom_original,
        "zoom_double" => hotkeys.zoom_double,
        _ => egui::Key::Space,
    }
}

fn set_key(hotkeys: &mut HotkeyConfig, action: &str, key: egui::Key) {
    match action {
        "play_pause" => hotkeys.play_pause = key,
        "seek_backward" => hotkeys.seek_backward = key,
        "seek_forward" => hotkeys.seek_forward = key,
        "seek_start" => hotkeys.seek_start = key,
        "seek_end" => hotkeys.seek_end = key,
        "frame_step" => hotkeys.frame_step = key,
        "frame_back_step" => hotkeys.frame_back_step = key,
        "seek_marker" => hotkeys.seek_marker = key,
        "mute" => hotkeys.mute = key,
        "zoom_quarter" => hotkeys.zoom_quarter = key,
        "zoom_half" => hotkeys.zoom_half = key,
        "zoom_original" => hotkeys.zoom_original = key,
        "zoom_double" => hotkeys.zoom_double = key,
        _ => {}
    }
}

fn find_conflict<'a>(hotkeys: &HotkeyConfig, action: &str, key: egui::Key) -> Option<String> {
    ACTIONS
        .iter()
        .find_map(|(id, _)| (*id != action && get_key(hotkeys, id) == key).then(|| id.to_string()))
}

// ── UI ───────────────────────────────────────────────────────────────────────

impl ConfigWindow {
    pub fn show(&mut self, ctx: &mut egui::Ui, hotkeys: &mut HotkeyConfig) {
        if !self.open {
            return;
        }

        self.handle_capture(ctx, hotkeys);
        self.show_conflict_popup(ctx, hotkeys);
        self.show_settings_window(ctx, hotkeys);
    }

    fn handle_capture(&mut self, ctx: &mut egui::Ui, hotkeys: &mut HotkeyConfig) {
        if self.capturing.is_none() {
            return;
        }

        let pressed = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = e
                {
                    return Some((*key != egui::Key::Escape).then_some(*key));
                }
                None
            })
        });

        let Some(maybe_key) = pressed else { return };
        let action = self.capturing.take().unwrap();

        if let Some(key) = maybe_key {
            match find_conflict(hotkeys, &action, key) {
                Some(conflicting) => {
                    self.conflict = Some(ConflictState {
                        action,
                        key,
                        conflicting,
                    });
                }
                None => {
                    set_key(hotkeys, &action, key);
                }
            }
        }
    }

    fn show_conflict_popup(&mut self, ctx: &mut egui::Ui, hotkeys: &mut HotkeyConfig) {
        let Some(conflict) = &self.conflict else {
            return;
        };

        let action = conflict.action.clone();
        let key = conflict.key;
        let conflicting = conflict.conflicting.clone();
        let mut resolve: Option<bool> = None;

        egui::Window::new("Key Conflict")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "{:?} is already assigned to \"{}\". Replace it?",
                    key, conflicting
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        resolve = Some(true);
                    }
                    if ui.button("No").clicked() {
                        resolve = Some(false);
                    }
                });
            });

        if let Some(yes) = resolve {
            if yes {
                let old_key = get_key(hotkeys, &action);
                set_key(hotkeys, &conflicting, old_key);
                set_key(hotkeys, &action, key);
            }
            self.conflict = None;
        }
    }

    fn show_settings_window(&mut self, ctx: &mut egui::Ui, hotkeys: &mut HotkeyConfig) {
        egui::Window::new("Settings")
            .open(&mut self.open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.add_visible(
                    self.capturing.is_some(),
                    egui::Label::new("Press any key... (Esc to cancel)"),
                );
                ui.separator();

                egui::Grid::new("hotkey_grid")
                    .num_columns(3)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        for (action, label) in ACTIONS {
                            let current = get_key(hotkeys, action);
                            Self::row(ui, &mut self.capturing, label, action, current);
                        }
                    });

                ui.separator();
                if ui.button("Reset to Default").clicked() {
                    *hotkeys = HotkeyConfig::default();
                    self.capturing = None;
                }
            });
    }

    fn row(
        ui: &mut egui::Ui,
        capturing: &mut Option<String>,
        label: &str,
        action: &str,
        current: egui::Key,
    ) {
        const KEY_COL_WIDTH: f32 = 150.0;

        ui.label(label);

        let height = ui.spacing().interact_size.y;
        if capturing.as_deref() == Some(action) {
            ui.add_sized(
                [KEY_COL_WIDTH, height],
                egui::Label::new("[ Press any key... ]"),
            );
            if ui.button("Cancel").clicked() {
                *capturing = None;
            }
        } else {
            ui.add_sized(
                [KEY_COL_WIDTH, height],
                egui::Label::new(key_label(current)),
            );
            if ui.button("Change").clicked() {
                *capturing = Some(action.to_string());
            }
        }

        ui.end_row();
    }
}
