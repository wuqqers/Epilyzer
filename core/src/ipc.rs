use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcCommand {
    SetBrightness(f64),
    SetWakeTime(u8, u8), // Hour, Minute
    SetTransitionDuration(u64), // Milliseconds
    SetFlashbangProtection(bool),
    GetInfo,
    Freeze(u64), // Seconds
    ResetAuto,
    Heartbeat,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Ok,
    Status {
        brightness: f64,
        location: String,
        wake_time: String,
        transition_duration_ms: u64,
        flashbang_protection: bool,
    },
    Error(String),
}
