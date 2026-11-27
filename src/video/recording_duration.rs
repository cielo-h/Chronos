//! Follow the Matroska timeline without decoding packets or requiring final Cues.
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

pub(super) struct RecordingDuration {
    _stop: mpsc::Sender<()>,
    updates: Receiver<f64>,
    latest: f64,
}

impl RecordingDuration {
    pub(super) fn watch(path: String) -> Self {
        let (stop, stopped) = mpsc::channel();
        let (updates, receiver) = mpsc::channel();
        thread::spawn(move || {
            let Ok(file) = File::open(path) else { return };
            let mut input = BufReader::new(file);
            let mut timeline = Timeline::default();
            let mut last_size = None;
            loop {
                let Ok(size) = input.get_ref().metadata().map(|m| m.len()) else {
                    break;
                };
                // Bound each batch so replacing a large recording cancels promptly.
                if last_size != Some(size) {
                    match timeline.scan(&mut input, size) {
                        Ok(true) => {
                            if stopped.try_recv() == Err(mpsc::TryRecvError::Empty) {
                                continue;
                            }
                            break;
                        }
                        Ok(false) => {
                            last_size = Some(size);
                            if updates.send(timeline.seconds()).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            log::warn!("Cannot read Matroska timeline: {error}");
                            break;
                        }
                    }
                }
                if stopped.recv_timeout(Duration::from_secs(1)) != Err(RecvTimeoutError::Timeout) {
                    break;
                }
            }
        });
        Self {
            _stop: stop,
            updates: receiver,
            latest: 0.0,
        }
    }

    pub(super) fn latest(&mut self) -> f64 {
        for seconds in self.updates.try_iter() {
            self.latest = self.latest.max(seconds);
        }
        self.latest
    }
}

#[derive(Default)]
struct Timeline {
    offset: u64,
    scale: Option<u64>,
    first_cluster: Option<u64>,
    cluster: u64,
    latest: u64,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid EBML element")
}

fn uint(r: &mut impl Read, bytes: usize) -> io::Result<u64> {
    let mut buf = [0; 8];
    if bytes > 8 {
        return Err(invalid());
    }
    r.read_exact(&mut buf[8 - bytes..])?;
    Ok(u64::from_be_bytes(buf))
}

fn vint(r: &mut impl Read, id: bool) -> io::Result<(u64, usize)> {
    let first = uint(r, 1)? as u8;
    let extra = first.leading_zeros() as usize;
    if extra >= 8 || (id && extra >= 4) {
        return Err(invalid());
    }
    let high = if id { first } else { first ^ (0x80 >> extra) };
    Ok((((high as u64) << (8 * extra)) | uint(r, extra)?, extra + 1))
}

impl Timeline {
    fn seconds(&self) -> f64 {
        self.latest.saturating_sub(self.first_cluster.unwrap_or(0)) as f64
            * self.scale.unwrap_or(1_000_000) as f64
            / 1e9
    }

    // Descend only into these Matroska master elements. Skip packet payloads
    // by their declared lengths so payload bytes cannot resemble a Cluster ID.
    fn scan(&mut self, r: &mut BufReader<impl Read + Seek>, size: u64) -> io::Result<bool> {
        r.seek(SeekFrom::Start(self.offset))?;
        for _ in 0..4096 {
            if self.offset >= size {
                return Ok(false);
            }
            let header: io::Result<_> = (|| Ok((vint(r, true)?.0, vint(r, false)?)))();
            let (id, (len, width)) = match header {
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
                result => result?,
            };
            let start = r.stream_position()?;
            if matches!(id, 0x18538067 | 0x1549a966 | 0x1f43b675 | 0xa0) {
                self.offset = start;
                continue;
            }
            if len == (1u64 << (7 * width)) - 1 {
                return Err(invalid());
            }
            let end = start.checked_add(len).ok_or_else(invalid)?;
            // Retry a partially written element on the next size change.
            if end > size {
                return Ok(false);
            }
            match id {
                0x2ad7b1 => self.scale = Some(uint(r, len as usize)?),
                0xe7 => {
                    self.cluster = uint(r, len as usize)?;
                    self.first_cluster.get_or_insert(self.cluster);
                }
                0xa3 | 0xa1 => {
                    let (_, track_width) = vint(r, false)?;
                    if len < track_width as u64 + 3 {
                        return Err(invalid());
                    }
                    let relative = uint(r, 2)? as i16;
                    self.latest = self
                        .latest
                        .max(self.cluster.saturating_add_signed(i64::from(relative)));
                }
                _ => {}
            }
            let position = r.stream_position()?;
            r.seek_relative(i64::try_from(end - position).map_err(|_| invalid())?)?;
            self.offset = end;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn resumes_inside_an_unfinished_block() {
        let mut bytes = vec![
            0x18, 0x53, 0x80, 0x67, 0xff, // Segment, unknown size
            0x15, 0x49, 0xa9, 0x66, 0x87, // Info
            0x2a, 0xd7, 0xb1, 0x83, 0x1e, 0x84, 0x80, // 2 ms per tick
            0x1f, 0x43, 0xb6, 0x75, 0xff, // Cluster, unknown size
            0xe7, 0x82, 0x03, 0xe8, // recording starts at tick 1000
            0xa3, 0x84, 0x81, 0x27, 0x10, 0x80, // block at +10000
            0x1f, 0x43, 0xb6, 0x75, 0xff, 0xe7, 0x82, 0x52,
            0x08, // next cluster at tick 21000
            0xa3, 0x87, 0x81, 0xff, 0xfb, 0x80, // block at -5; payload to come
        ];
        let mut timeline = Timeline::default();
        let mut input = BufReader::new(Cursor::new(&bytes[..]));
        assert!(!timeline.scan(&mut input, bytes.len() as u64).unwrap());
        assert_eq!(timeline.seconds(), 20.0);
        bytes.extend_from_slice(&[0xe7, 0x81, 0xff]); // payload resembles a timestamp
        let mut input = BufReader::new(Cursor::new(&bytes[..]));
        assert!(!timeline.scan(&mut input, bytes.len() as u64).unwrap());
        assert!((timeline.seconds() - 39.99).abs() < 1e-9);
    }
}
