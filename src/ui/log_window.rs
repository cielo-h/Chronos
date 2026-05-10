use crate::logger::global_memory_logger;
use eframe::egui::{self, Color32};
use egui_extras::{Column, TableBuilder};
use std::process::Command;

pub struct LogWindow {
    pub open: bool,
    filter_query: String,
}

impl LogWindow {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
    fn open_log_file() {
        let Ok(path) = crate::logger::log_path() else { return };
        let Some(dir) = path.parent() else { return };

        #[cfg(target_os = "windows")]
        let _ = Command::new("explorer").arg(dir).spawn();

        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(dir).spawn();

        #[cfg(target_os = "linux")]
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}

impl LogWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            filter_query: String::new(),
        }
    }

    fn set_visuals(&mut self, ui: &mut egui::Ui) {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::WHITE);
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(60, 60, 60));
        visuals.panel_fill = Color32::from_rgb(30, 30, 30);
        ui.set_visuals(visuals);
    }

    pub fn ui_content(&mut self, ui: &mut egui::Ui) {
        self.set_visuals(ui);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(
                egui_phosphor::regular::MAGNIFYING_GLASS,
            ));
            ui.text_edit_singleline(&mut self.filter_query);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("ログフォルダを開く").clicked() {
                    Self::open_log_file();
                }
                if ui.button("クリア").clicked() {
                    if let Ok(mut entries) = global_memory_logger().entries().lock() {
                        entries.clear();
                    }
                }
            });
        });

        ui.separator();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .stick_to_bottom(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(55.0).resizable(false))
            .column(Column::initial(60.0).resizable(false))
            .column(Column::remainder())
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("時刻");
                });
                header.col(|ui| {
                    ui.strong("レベル");
                });
                header.col(|ui| {
                    ui.strong("メッセージ");
                });
            })
            .body(|body| {
                if let Ok(entries) = global_memory_logger().entries().lock() {
                    let filtered_entries: Vec<_> = entries
                        .iter()
                        .filter(|e| {
                            self.filter_query.is_empty()
                                || e.message
                                    .to_lowercase()
                                    .contains(&self.filter_query.to_lowercase())
                        })
                        .collect();

                    body.rows(18.0, filtered_entries.len(), |mut row| {
                        let row_index = row.index();
                        let entry = filtered_entries[row_index];
                        row.set_selected(false);

                        let color = match entry.level {
                            log::Level::Error => Color32::LIGHT_RED,
                            log::Level::Warn => Color32::GOLD,
                            log::Level::Info => Color32::LIGHT_BLUE,
                            _ => Color32::GRAY,
                        };

                        row.col(|ui| {
                            ui.label(&entry.timestamp);
                        });
                        row.col(|ui| {
                            ui.colored_label(color, entry.level.to_string());
                        });
                        row.col(|ui| {
                            ui.colored_label(color, &entry.message);
                        });
                    });
                }
            });
    }
}
