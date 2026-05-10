use anyhow::{Context, Result};
use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

pub const MAX_LOG_ENTRIES: usize = 200;

#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: log::Level,
    pub message: String,
}

pub struct MemoryLogger {
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl MemoryLogger {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::with_capacity(MAX_LOG_ENTRIES))),
        }
    }

    pub fn entries(&self) -> Arc<Mutex<Vec<LogEntry>>> {
        Arc::clone(&self.entries)
    }
}

struct AppLogger {
    memory: MemoryLogger,
    file_mutex: Mutex<fs::File>,
}

impl Log for AppLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let now = Local::now().format("%H:%M:%S").to_string();
            let msg = format!("{}", record.args());

            let file_line = format!("[{}] {} {}\n", now, record.level(), msg);
            if let Ok(mut file) = self.file_mutex.lock() {
                let _ = file.write_all(file_line.as_bytes());
            }

            if let Ok(mut entries) = self.memory.entries().lock() {
                if entries.len() >= MAX_LOG_ENTRIES {
                    entries.remove(0);
                }
                entries.push(LogEntry {
                    timestamp: now,
                    level: record.level(),
                    message: msg,
                });
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file_mutex.lock() {
            let _ = file.flush();
        }
    }
}

pub fn global_memory_logger() -> &'static MemoryLogger {
    static INSTANCE: OnceLock<MemoryLogger> = OnceLock::new();
    INSTANCE.get_or_init(MemoryLogger::new)
}

pub fn init_logger() -> Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path()?)
        .context("Failed to open app.log")?;

    let logger = AppLogger {
        memory: MemoryLogger {
            entries: global_memory_logger().entries(),
        },
        file_mutex: Mutex::new(file),
    };

    log::set_boxed_logger(Box::new(logger))
        .map(|()| log::set_max_level(LevelFilter::Debug))
        .context("Failed to set logger")?;

    Ok(())
}

pub fn log_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("Failed to find log directory")?
        .join(crate::APP_NAME);

    fs::create_dir_all(&dir).context("Failed to create log directory")?;

    Ok(dir.join(format!("{}.log", crate::APP_NAME.to_lowercase())))
}
