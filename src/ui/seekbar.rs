//! カスタムシークバーウィジェット

use crate::video::mpv_manager::MpvManager;
use crate::video::thumbnail::ThumbnailManager;
use eframe::egui::{self, Color32, Pos2, Response, Sense, Stroke, Ui, Widget};

pub struct SeekbarWidget<'a> {
    mpv: &'a MpvManager,
    seek_request: &'a mut Option<f64>,
    marker_request: &'a mut Option<f64>,
    thumbnail_manager: &'a mut Option<ThumbnailManager>,
    drag_pos: &'a mut Option<f64>,
    last_seek_time: &'a mut f64,
}

impl<'a> SeekbarWidget<'a> {
    pub fn new(
        player: &'a MpvManager,
        seek_request: &'a mut Option<f64>,
        marker_request: &'a mut Option<f64>,
        thumbnail_manager: &'a mut Option<ThumbnailManager>,
        drag_pos: &'a mut Option<f64>,
        last_seek_time: &'a mut f64,
    ) -> Self {
        Self {
            mpv: player,
            seek_request,
            marker_request,
            thumbnail_manager,
            drag_pos,
            last_seek_time,
        }
    }

    fn format_time(sec: f64) -> String {
        let h = (sec / 3600.0).floor() as u32;
        let m = ((sec % 3600.0) / 60.0).floor() as u32;
        let s = sec % 60.0;
        format!("{:02}:{:02}:{:06.3}", h, m, s)
    }
}

impl<'a> Widget for SeekbarWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let height = 24.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            Sense::click_and_drag(),
        );

        let duration = if self.mpv.duration > 0.0 {
            self.mpv.duration
        } else {
            1.0
        };

        // シークロジック
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let t = (ratio as f64) * duration;
                *self.drag_pos = Some(t);

                let now = ui.input(|i| i.time);
                if now - *self.last_seek_time > 0.6 {
                    self.mpv.seek_keyframes(t);
                    *self.last_seek_time = now;
                }
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(t) = self.drag_pos.take() {
                *self.seek_request = Some(t);
            }
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                *self.seek_request = Some((ratio as f64) * duration);
            }
        }
        
        if response.clicked_by(egui::PointerButton::Secondary) {
            if let Some(pos) = response
                .interact_pointer_pos()
                .or_else(|| response.hover_pos())
            {
                let ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                *self.marker_request = Some((ratio as f64) * duration);
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            
            let bar_y = rect.center().y;
            painter.line_segment(
                [
                    Pos2::new(rect.left(), bar_y),
                    Pos2::new(rect.right(), bar_y),
                ],
                Stroke::new(5.0, Color32::from_rgb(80, 80, 80)),
            );
            
            let display_time = self.drag_pos.unwrap_or(self.mpv.current_time);
            let current_x = rect.left() + (display_time / duration) as f32 * rect.width();
            painter.line_segment(
                [Pos2::new(rect.left(), bar_y), Pos2::new(current_x, bar_y)],
                Stroke::new(5.0, Color32::LIGHT_BLUE),
            );
            
            let marker_x = rect.left() + (self.mpv.marker_time / duration) as f32 * rect.width();
            painter.line_segment(
                [
                    Pos2::new(marker_x, rect.top()),
                    Pos2::new(marker_x, rect.bottom()),
                ],
                Stroke::new(2.0, Color32::RED),
            );

            painter.circle_filled(Pos2::new(current_x, bar_y), 6.0, Color32::WHITE);

            if response.hovered() {
                if let Some(pos) = response.hover_pos() {
                    let ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let hover_time = (ratio as f64) * duration;
                    
                    egui::Tooltip::for_enabled(&response)
                        .at_pointer()
                        .show(|ui| {
                            ui.label(Self::format_time(hover_time));

                            if let Some(tm) = self.thumbnail_manager {
                                tm.request_thumbnail(hover_time);

                                if let Some(texture) = tm.get_texture(hover_time, ui.ctx()) {
                                    ui.add(
                                        egui::Image::new(&texture)
                                            .fit_to_exact_size(egui::vec2(213.0, 120.0)),
                                    );
                                } else {
                                    ui.allocate_space(egui::vec2(213.0, 120.0));
                                }
                            }
                        });
                    
                    if let Some(tm) = self.thumbnail_manager {
                        let _ = tm.request_thumbnail(hover_time);
                    }
                }
            }
        }
        response
    }
}
