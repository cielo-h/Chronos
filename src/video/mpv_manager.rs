use crate::config::AppConfig;
use crate::video::render_context::GpuRenderContext;
use crate::video::render_context::MpvError;
use libmpv2::Mpv;
use std::ffi::{CStr, c_void};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};

const INIT_W: u32 = 1920;
const INIT_H: u32 = 1080;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ManagerError {
    MpvInit(String),
    RenderContext(MpvError),
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::MpvInit(msg) => write!(f, "mpv init failed: {msg}"),
            ManagerError::RenderContext(e) => write!(f, "render context failed: {e}"),
        }
    }
}

impl std::error::Error for ManagerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ManagerError::RenderContext(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MpvError> for ManagerError {
    fn from(e: MpvError) -> Self {
        ManagerError::RenderContext(e)
    }
}

#[derive(Debug)]
pub enum MpvCommand {
    LoadFile(String),
    Seek(f64),
    SetVolume(u32),
    SetPause(bool),
    TogglePause,
    SetMute(bool),
    ToggleMute,
}

pub struct MpvManager {
    pub render_ctx: GpuRenderContext,
    pub mpv: Mpv,

    cmd_tx: Sender<MpvCommand>,
    cmd_rx: Receiver<MpvCommand>,

    vid_w: u32,
    vid_h: u32,

    pub marker_time: f64,
    pub duration: f64,
    pub current_time: f64,

    pub is_loaded: bool,
    pub is_playing: bool,
    pub is_muted: bool,
    pub volume: u32,
}

impl MpvManager {
    pub fn new(
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        config: &AppConfig,
    ) -> Result<Self, ManagerError> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("hwdec", "no")?;
            init.set_property("volume-max", "100")?;
            init.set_property("keep-open", "yes")?;
            Ok(())
        })
        .expect("mpv initialize failed");

        let frame_ready = Arc::new(AtomicBool::new(false));

        let render_ctx = GpuRenderContext::new(
            &mpv,
            Arc::clone(&frame_ready),
            get_proc_address,
            INIT_W,
            INIT_H,
        )
        .expect("gpu render context created failed");

        mpv.set_property("volume", Self::ui_to_mpv_vol(config.volume))
            .ok();

        let (cmd_tx, cmd_rx) = mpsc::channel();

        Ok(Self {
            mpv,
            render_ctx,
            cmd_tx,
            cmd_rx,
            vid_w: INIT_W,
            vid_h: INIT_H,
            marker_time: 0.0,
            duration: 0.0,
            current_time: 0.0,
            is_loaded: false,
            is_playing: false,
            is_muted: false,
            volume: config.volume,
        })
    }

    // ── command channel ──────────────────────────────────────────────────

    pub fn sender(&self) -> Sender<MpvCommand> {
        self.cmd_tx.clone()
    }

    // ── direct api ──────────────────────────────────────────────────────────

    pub fn load_file(&self, path: &str) {
        if path.is_empty() {
            return;
        }

        let at_eof = self
            .mpv
            .get_property::<bool>("eof_reached")
            .unwrap_or(false);
        if at_eof {
            self.mpv.set_property("pause", false).ok();
        }

        self.mpv
            .command("loadfile", &[path, "replace", "0", "start=0"])
            .ok();
    }

    pub fn close_file(&self) {
        self.mpv.command("stop", &[]).ok();
    }

    pub fn seek(&self, secs: f64) {
        let s = secs.to_string();
        self.mpv.command("seek", &[&s, "relative+exact"]).ok();
    }

    pub fn seek_absolute(&mut self, secs: f64) {
        let s = secs.to_string();
        self.mpv.command("seek", &[&s, "absolute+exact"]).ok();
    }

    pub fn seek_keyframes(&self, secs: f64) {
        let s = secs.to_string();
        self.mpv
            .command("seek", &[&s, "absolute+keyframes+exact"])
            .ok();
    }

    pub fn seek_start(&mut self) {
        self.mpv.command("seek", &["0", "absolute+exact"]).ok();
    }

    pub fn seek_end(&mut self) {
        if let Ok(dur) = self.mpv.get_property::<f64>("duration") {
            let end = (dur - 0.1).max(0.0);
            let s = end.to_string();
            self.mpv.command("seek", &[&s, "absolute+exact"]).ok();
        }
    }

    pub fn toggle_pause(&self) {
        let at_eof = self
            .mpv
            .get_property::<bool>("eof-reached")
            .unwrap_or(false);
        if at_eof {
            self.mpv.command("seek", &["0", "absolute"]).ok();
            self.mpv.set_property("pause", false).ok();
        } else {
            let paused = self.mpv.get_property::<bool>("pause").unwrap_or(false);
            self.mpv.set_property("pause", !paused).ok();
        }
    }

    pub fn set_pause(&self, paused: bool) {
        self.mpv.set_property("pause", paused).ok();
    }

    pub fn get_volume(&self) -> u32 {
        let raw = self.mpv.get_property::<f64>("volume").unwrap_or(40.0);
        Self::mpv_to_ui_vol(raw)
    }

    pub fn set_volume(&self, vol: u32) {
        self.mpv
            .set_property("volume", Self::ui_to_mpv_vol(vol))
            .ok();
    }

    pub fn toggle_mute(&mut self) {
        let muted = self.mpv.get_property::<bool>("mute").unwrap_or(false);
        let new_muted = !muted;
        self.mpv.set_property("mute", new_muted).ok();
        self.is_muted = new_muted;
    }

    pub fn frame_step(&self) {
        let paused = self.mpv.get_property::<bool>("pause").unwrap_or(true);
        if paused {
            self.mpv.command("frame-step", &[]).ok();
        }
    }

    pub fn frame_back_step(&self) {
        let was_playing = !self.mpv.get_property::<bool>("pause").unwrap_or(true);
        self.mpv.command("frame-back-step", &[]).ok();
        if was_playing {
            self.mpv.set_property("pause", false).ok();
        }
    }

    pub fn set_mute(&self, muted: bool) {
        self.mpv.set_property("mute", muted).ok();
    }

    // ── Get state ──────────────────────────────────────────────────────────────

    /// sync current state
    fn sync_state(&mut self) {
        self.is_loaded = !self.mpv.get_property::<bool>("idle-active").unwrap_or(true);
        self.current_time = self.mpv.get_property::<f64>("time-pos").unwrap_or(0.0);
        self.duration = self.mpv.get_property::<f64>("duration").unwrap_or(0.0);
        self.is_playing = !self.mpv.get_property::<bool>("pause").unwrap_or(false);
        self.is_muted = self.mpv.get_property::<bool>("mute").unwrap_or(false);
        self.volume = {
            let raw = self.mpv.get_property::<f64>("volume").unwrap_or(40.0);
            Self::mpv_to_ui_vol(raw)
        };
    }

    /// Get current video size
    pub fn video_size(&self) -> egui::Vec2 {
        egui::Vec2::new(self.vid_w as f32, self.vid_h as f32)
    }

    pub fn update_frame(&mut self) {
        self.sync_video_size();
        self.sync_state();
        self.render_ctx.render_frame();
    }

    /// The OpenGL texture ID that holds the latest rendered video frame.
    pub fn video_texture_id(&self) -> u32 {
        self.render_ctx.fbo_tex
    }

    /// Returns a reference to the shared frame-ready flag.
    pub fn frame_ready(&self) -> &Arc<AtomicBool> {
        &self.render_ctx.frame_ready()
    }

    // ── Private ──────────────────────────────────────────────────────────

    fn ui_to_mpv_vol(ui_vol: u32) -> f64 {
        ((ui_vol as f64 / 100.0).sqrt() * 100.0).min(100.0)
    }

    fn mpv_to_ui_vol(mpv_vol: f64) -> u32 {
        ((mpv_vol / 100.0).powi(2) * 100.0).round() as u32
    }

    /// Process commands
    fn drain_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            match cmd {
                MpvCommand::LoadFile(path) => self.load_file(&path),
                MpvCommand::Seek(secs) => self.seek(secs),
                MpvCommand::SetVolume(v) => self.set_volume(v),
                MpvCommand::SetPause(p) => self.set_pause(p),
                MpvCommand::TogglePause => self.toggle_pause(),
                MpvCommand::SetMute(m) => self.set_mute(m),
                MpvCommand::ToggleMute => self.toggle_mute(),
            }
        }
    }

    /// Resize the render context to match the video's reported dimensions.
    fn sync_video_size(&mut self) {
        let w = self
            .mpv
            .get_property::<i64>("width")
            .unwrap_or(INIT_W as i64);
        let h = self
            .mpv
            .get_property::<i64>("height")
            .unwrap_or(INIT_H as i64);

        // Guard against degenerate sizes (e.g. during seek or before first frame).
        if w > 0 && h > 0 {
            self.render_ctx.resize(w as u32, h as u32);
            self.vid_w = w as u32;
            self.vid_h = h as u32;
        }
    }
}
