use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::format_unix_ms_utc;

pub struct Diagnostics {
    file: Option<File>,
}

impl Diagnostics {
    pub fn disabled() -> Self {
        Self { file: None }
    }

    pub fn append_to_path(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open MCP log file {}", path.display()))?;
        Ok(Self { file: Some(file) })
    }

    pub fn info(&mut self, message: impl AsRef<str>) {
        self.write("INFO", message.as_ref());
    }

    pub fn warn(&mut self, message: impl AsRef<str>) {
        self.write("WARN", message.as_ref());
    }

    pub fn error(&mut self, message: impl AsRef<str>) {
        self.write("ERROR", message.as_ref());
    }

    fn write(&mut self, level: &str, message: &str) {
        let Some(file) = &mut self.file else {
            return;
        };
        let timestamp = format_unix_ms_utc(now_ms());
        if let Err(err) = writeln!(file, "{timestamp} {level} {message}") {
            eprintln!("Warning: Failed to write MCP log file: {err}");
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
