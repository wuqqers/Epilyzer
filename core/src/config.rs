use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;
use crate::epilepsy::{MIN_TRANSITION_TIME_SEC, MAX_CHANGE_FREQUENCY_HZ};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse Error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Validation Error: {0}")]
    Validation(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub general: GeneralConfig,
    pub location: LocationConfig,
    pub epilepsy_protection: EpilepsyConfig,
    pub brightness: BrightnessConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GeneralConfig {
    pub enabled: bool,
    pub mode: String, // "normal", "safe", "sleep"
    pub log_level: String,
    #[serde(default = "default_wake_time")]
    pub wake_time: String, // "HH:MM"
}

fn default_wake_time() -> String {
    "07:00".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationConfig {
    pub method: String, // "auto", "gps", "ip", "manual"
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub timezone: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EpilepsyConfig {
    pub enabled: bool,
    pub min_transition_time: f64,
    pub max_changes_per_second: f64,
    pub smooth_steps: u32,
    pub emergency_hotkey: String,
    pub safe_mode_brightness: f64,
    #[serde(default = "default_transition_duration_ms")]
    pub transition_duration_ms: u64,
}

fn default_transition_duration_ms() -> u64 {
    750
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BrightnessConfig {
    pub method: String, // "ddcutil", "backlight"
    pub min_brightness: f64,
    pub max_brightness: f64,
    pub default_brightness: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                enabled: true,
                mode: "normal".to_string(),
                log_level: "info".to_string(),
                wake_time: "07:00".to_string(),
            },
            location: LocationConfig {
                method: "auto".to_string(),
                latitude: Some(41.0082),
                longitude: Some(28.9784),
                timezone: "Europe/Istanbul".to_string(),
            },
            epilepsy_protection: EpilepsyConfig {
                enabled: true,
                min_transition_time: MIN_TRANSITION_TIME_SEC,
                max_changes_per_second: MAX_CHANGE_FREQUENCY_HZ,
                smooth_steps: 50,
                emergency_hotkey: "Ctrl+Alt+B".to_string(),
                safe_mode_brightness: 40.0,
                transition_duration_ms: 750,
            },
            brightness: BrightnessConfig {
                method: "ddcutil".to_string(),
                min_brightness: 15.0,
                max_brightness: 95.0,
                default_brightness: 50.0,
            },
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        
        // Basic validation
        if config.epilepsy_protection.min_transition_time < 0.5 {
             return Err(ConfigError::Validation("Transition time too short for safety".to_string()));
        }
        
        Ok(config)
    }
}
