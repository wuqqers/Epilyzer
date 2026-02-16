use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use tracing::{info, error};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppState {
    pub brightness: f64,
    pub wake_time: Option<(u8, u8)>,
    #[serde(default = "default_transition_duration")]
    pub transition_duration_ms: u64,
    #[serde(default = "default_flashbang")]
    pub flashbang_protection: bool,

    pub last_updated: chrono::DateTime<chrono::Utc>,
}

fn default_transition_duration() -> u64 {
    750
}

fn default_flashbang() -> bool {
    true
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            brightness: 15.0, // Default to safe dim level
            wake_time: None,
            transition_duration_ms: 750,
            flashbang_protection: true,

            last_updated: chrono::Utc::now(),
        }
    }
}

pub struct StateManager {
    path: PathBuf,
}

impl StateManager {
    pub fn new() -> Self {
        let path = dirs::data_dir()
            .unwrap_or(PathBuf::from("/tmp"))
            .join("auto_brightness_state.json");
        Self { path }
    }

    pub fn load(&self) -> AppState {
        if self.path.exists() {
            match fs::read_to_string(&self.path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(state) => {
                        info!("State loaded: {:?}", state);
                        return state;
                    }
                    Err(e) => error!("Failed to parse state file: {}", e),
                },
                Err(e) => error!("Failed to read state file: {}", e),
            }
        }
        info!("No valid state found, using default.");
        AppState::default()
    }

    pub fn save(&self, brightness: f64, wake_time: Option<(u8, u8)>, transition_duration_ms: u64, flashbang_protection: bool) {
        let state = AppState {
            brightness,
            wake_time,
            transition_duration_ms,
            flashbang_protection,
            last_updated: chrono::Utc::now(),
        };

        match serde_json::to_string(&state) {
            Ok(content) => {
                if let Err(e) = fs::write(&self.path, content) {
                    error!("Failed to write state file: {}", e);
                }
            }
            Err(e) => error!("Failed to serialize state: {}", e),
        }
    }
}
