use anyhow::{Context, Result};
use eframe::egui;
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags as ScalerFlags};
use ffmpeg_next as ffmpeg;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub struct ThumbnailManager {
    request_tx: Sender<f64>,
    response_rx: Receiver<(f64, egui::ColorImage)>,
    cache: HashMap<i64, egui::TextureHandle>,
    keys_queue: VecDeque<i64>,
    last_requested: i64,
}

impl ThumbnailManager {
    pub fn new(path: String) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<f64>();
        let (res_tx, res_rx) = mpsc::channel();

        thread::spawn(move || {
            let init_result = (|| -> Result<_> {
                let ictx = ffmpeg::format::input(&path).context("Failed to read file")?;
                let duration = ictx.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64;

                let video_stream = ictx
                    .streams()
                    .best(ffmpeg::media::Type::Video)
                    .context("No video stream")?;
                let stream_idx = video_stream.index();

                let context =
                    ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
                let decoder = context.decoder().video()?;

                let scaler = Scaler::get(
                    decoder.format(),
                    decoder.width(),
                    decoder.height(),
                    Pixel::RGB24,
                    213,
                    120, // resolution 213 * 120
                    ScalerFlags::BILINEAR,
                )?;

                Ok((ictx, stream_idx, decoder, scaler, duration))
            })();

            if let Ok((mut ictx, stream_idx, mut decoder, mut scaler, duration)) = init_result {
                let mut receive_frame = ffmpeg::frame::Video::empty();
                let mut rgb_frame = ffmpeg::frame::Video::empty();

                while let Ok(mut time) = req_rx.recv() {
                    while let Ok(newer_time) = req_rx.try_recv() {
                        time = newer_time;
                    }

                    if time > duration - 0.5 {
                        time = (duration - 0.5).max(0.0);
                    }

                    let seek_pos = (time * ffmpeg::ffi::AV_TIME_BASE as f64) as i64;
                    let _ = ictx.seek(seek_pos, ..);
                    decoder.flush();

                    let mut frame_found = false;

                    for (stream, packet) in ictx.packets() {
                        if stream.index() == stream_idx {
                            if decoder.send_packet(&packet).is_ok() {
                                if decoder.receive_frame(&mut receive_frame).is_ok() {
                                    frame_found = true;
                                    break;
                                }
                            }
                        }
                    }

                    if frame_found {
                        if scaler.run(&receive_frame, &mut rgb_frame).is_ok() {
                            let width = 213;
                            let height = 120;
                            let stride = rgb_frame.stride(0);
                            let line_size = width * 3;
                            let data_slice = rgb_frame.data(0);
                            let mut data = Vec::with_capacity(width * height * 3);

                            if stride == line_size {
                                data.extend_from_slice(&data_slice[..line_size * height]);
                            } else {
                                for y in 0..height {
                                    let start = y * stride;
                                    let end = start + line_size;
                                    data.extend_from_slice(&data_slice[start..end]);
                                }
                            }

                            let image = egui::ColorImage::from_rgb([width, height], &data);
                            let _ = res_tx.send((time, image));
                        }
                    }
                }
            }
        });

        Self {
            request_tx: req_tx,
            response_rx: res_rx,
            cache: HashMap::new(),
            keys_queue: VecDeque::new(),
            last_requested: -100,
        }
    }

    pub fn request_thumbnail(&mut self, time: f64) {
        let key = time.round() as i64;
        if !self.cache.contains_key(&key) && self.last_requested != key {
            self.last_requested = key;
            let _ = self.request_tx.send(time);
        }
    }

    pub fn get_texture(&mut self, time: f64, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        while let Ok((t, image)) = self.response_rx.try_recv() {
            let k = t.round() as i64;
            let tex = ctx.load_texture(format!("thumb_{}", k), image, egui::TextureOptions::LINEAR);

            if !self.cache.contains_key(&k) {
                self.keys_queue.push_back(k);
            }
            self.cache.insert(k, tex);

            while self.keys_queue.len() > 100 {
                if let Some(old_key) = self.keys_queue.pop_front() {
                    self.cache.remove(&old_key);
                }
            }
        }

        let key = time.round() as i64;
        self.cache.get(&key).cloned()
    }
}
