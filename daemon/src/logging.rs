use std::fs::OpenOptions;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::Serialize;
use anyhow::Result;

#[derive(Serialize)]
struct LogEntry {
    timestamp: DateTime<Utc>,
    event_type: String, // "override", "auto", "mode_change"
    brightness: f64,
    mode: String,
}

pub struct DataLogger {
    file_path: PathBuf,
}

impl DataLogger {
    pub fn new() -> Self {
        let path = dirs::data_dir()
            .unwrap_or(PathBuf::from("/tmp"))
            .join("auto_brightness_history.csv");
            
        Self { file_path: path }
    }

    pub fn log(&self, event_type: &str, brightness: f64, mode: &str) -> Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        let mut wtr = csv::WriterBuilder::new()
            .has_headers(file.metadata()?.len() == 0) // Write headers if empty
            .from_writer(file);

        wtr.serialize(LogEntry {
            timestamp: Utc::now(),
            event_type: event_type.to_string(),
            brightness,
            mode: mode.to_string(),
        })?;
        
        wtr.flush()?;
        Ok(())
    }
}
