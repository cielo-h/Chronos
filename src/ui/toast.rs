use ::eframe::egui;
use egui_phosphor::regular;
use std::time::Instant;

pub enum ToastKind {
    Success,
    Error,
    Warning,
    Info,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub start_time: Instant,
}

impl Toast {
    pub fn new(text: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            text: text.into(),
            kind,
            start_time: Instant::now(),
        }
    }

    fn style(&self) -> (egui::Color32, egui::Color32, String) {
        match self.kind {
            ToastKind::Success => (
                egui::Color32::from_rgb(40, 120, 40),
                egui::Color32::WHITE,
                format!("[{}]", regular::CHECK_FAT),
            ),
            ToastKind::Error => (
                egui::Color32::from_rgb(180, 40, 40),
                egui::Color32::WHITE,
                format!("[{}]", regular::EXCLAMATION_MARK),
            ),
            ToastKind::Warning => (
                egui::Color32::from_rgb(200, 150, 0),
                egui::Color32::BLACK,
                format!("[{}]", regular::QUESTION),
            ),
            ToastKind::Info => (
                egui::Color32::from_rgb(40, 80, 150),
                egui::Color32::WHITE,
                format!("[{}]", regular::INFO),
            ),
        }
    }

    pub fn show(&self, ctx: &egui::Context) {
        let (bg_color, text_color, icon) = self.style();

        egui::Area::new(egui::Id::new("toast_area"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(bg_color)
                    .corner_radius(3.0)
                    .shadow(egui::Shadow::default())
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon).color(text_color).strong());
                            ui.label(egui::RichText::new(&self.text).color(text_color).strong());
                        });
                    });
            });
    }
}
