use std::process::Command;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum HardwareError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Not supported")]
    NotSupported,
    #[error("Value out of range")]
    OutOfRange,
}

pub trait BrightnessController {
    fn get_brightness(&self) -> Result<f64, HardwareError>;
    fn set_brightness(&mut self, value: f64) -> Result<(), HardwareError>;
    fn name(&self) -> &str;
}



pub struct DdcUtilController {
    display_id: u8,
}

impl DdcUtilController {
    pub fn new(display_id: u8) -> Self {
        Self { display_id }
    }
}

impl BrightnessController for DdcUtilController {
    fn get_brightness(&self) -> Result<f64, HardwareError> {
        let output = Command::new("ddcutil")
            .args(&["getvcp", "10", "--display", &self.display_id.to_string(), "--brief"])
            .output()?;
            
        if !output.status.success() {
             return Err(HardwareError::CommandFailed("ddcutil get failed".to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split_whitespace().collect();
        if parts.len() >= 4 {
             if let Ok(val) = parts[3].parse::<f64>() {
                 return Ok(val);
             }
        }
        
        Err(HardwareError::CommandFailed("Could not parse ddcutil output".to_string()))
    }

    fn set_brightness(&mut self, value: f64) -> Result<(), HardwareError> {
        let val_int = value.round() as i32;
        info!("DDC set brightness: {}", val_int);
        
        let status = Command::new("ddcutil")
            .args(&["setvcp", "10", &val_int.to_string(), "--display", &self.display_id.to_string()])
            .status()?;
            
        if status.success() {
            Ok(())
        } else {
            Err(HardwareError::CommandFailed("ddcutil set failed".to_string()))
        }
    }

    fn name(&self) -> &str {
        "DDC/CI"
    }
}

pub struct BacklightController {
    device_path: PathBuf,
    max_brightness: f64,
}

impl BacklightController {
    pub fn new(name: &str) -> Result<Self, HardwareError> {
        let base = PathBuf::from("/sys/class/backlight").join(name);
        if !base.exists() {
             return Err(HardwareError::NotSupported);
        }
        
        // Critically, we must check if we can WRITE to the brightness file.
        // If not (e.g. user hasn't relogged for udev rule), we should fail
        // so the daemon falls back to KDE/DBus controller.
        let brightness_path = base.join("brightness");
        if let Err(_) = fs::OpenOptions::new().write(true).open(&brightness_path) {
            warn!("Found backlight device '{}' but cannot write to it (Permission Denied). Falling back...", name);
            return Err(HardwareError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
        }

        let max_str = fs::read_to_string(base.join("max_brightness"))?;
        let max_brightness = max_str.trim().parse::<f64>().map_err(|_| HardwareError::NotSupported)?;
        
        Ok(Self {
            device_path: base,
            max_brightness,
        })
    }
    
    pub fn auto() -> Result<Self, HardwareError> {
        let entries = fs::read_dir("/sys/class/backlight")?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                // Try to initialize. New logic will fail if not writable.
                if let Ok(c) = Self::new(name_str) {
                    return Ok(c);
                }
            }
        }
        Err(HardwareError::NotSupported)
    }
}

impl BrightnessController for BacklightController {
    fn get_brightness(&self) -> Result<f64, HardwareError> {
        let path = self.device_path.join("brightness");
        let content = fs::read_to_string(path)?;
        let val = content.trim().parse::<f64>().map_err(|_| HardwareError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid float")))?;
        
        Ok((val / self.max_brightness) * 100.0)
    }

    fn set_brightness(&mut self, value: f64) -> Result<(), HardwareError> {
        let val_clamped = value.clamp(0.0, 100.0);
        let raw_val = (val_clamped / 100.0 * self.max_brightness).round() as i32;
        
        fs::write(self.device_path.join("brightness"), raw_val.to_string())?;
        
        Ok(())
    }

    fn name(&self) -> &str {
        "Backlight (sysfs)"
    }
}

pub struct DummyController {
    brightness: f64,
}

impl DummyController {
    pub fn new() -> Self {
        Self { brightness: 50.0 }
    }
}

impl BrightnessController for DummyController {
    fn get_brightness(&self) -> Result<f64, HardwareError> {
        Ok(self.brightness)
    }

    fn set_brightness(&mut self, value: f64) -> Result<(), HardwareError> {
        self.brightness = value;
        info!("Dummy set brightness: {}", value);
        Ok(())
    }

    fn name(&self) -> &str {
        "Dummy"
    }
}

pub struct KdeBrightnessController {
    connection: zbus::blocking::Connection,
}

impl KdeBrightnessController {
    pub fn new() -> Result<Self, HardwareError> {
        // Estabilish persistent connection
        let connection = zbus::blocking::Connection::session()
            .map_err(|e| HardwareError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            
        // Test connection by reading max brightness
        // org.kde.Solid.PowerManagement.Actions.BrightnessControl.brightnessMax
        let _max: i32 = connection.call_method(
            Some("org.kde.Solid.PowerManagement"),
            "/org/kde/Solid/PowerManagement/Actions/BrightnessControl",
            Some("org.kde.Solid.PowerManagement.Actions.BrightnessControl"),
            "brightnessMax",
            &(),
        ).map_err(|_| HardwareError::NotSupported)?.body().deserialize().map_err(|_| HardwareError::NotSupported)?;

        Ok(Self { connection })
    }
    
    fn get_max(&self) -> Result<i32, HardwareError> {
        let max: i32 = self.connection.call_method(
            Some("org.kde.Solid.PowerManagement"),
            "/org/kde/Solid/PowerManagement/Actions/BrightnessControl",
            Some("org.kde.Solid.PowerManagement.Actions.BrightnessControl"),
            "brightnessMax",
            &(),
        )
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Error: {}", e)))?
        .body().deserialize()
        .map_err(|e| HardwareError::CommandFailed(format!("Parse Error: {}", e)))?;
        
        Ok(max)
    }
}

impl BrightnessController for KdeBrightnessController {
    fn get_brightness(&self) -> Result<f64, HardwareError> {
        let val: i32 = self.connection.call_method(
            Some("org.kde.Solid.PowerManagement"),
            "/org/kde/Solid/PowerManagement/Actions/BrightnessControl",
            Some("org.kde.Solid.PowerManagement.Actions.BrightnessControl"),
            "brightness",
            &(),
        )
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Error: {}", e)))?
        .body().deserialize()
        .map_err(|e| HardwareError::CommandFailed(format!("Parse Error: {}", e)))?;
        
        let max = self.get_max()?;
        if max == 0 { return Ok(0.0); }
        
        Ok((val as f64 / max as f64) * 100.0)
    }

    fn set_brightness(&mut self, value: f64) -> Result<(), HardwareError> {
        let max = self.get_max()?;
        let target = (value / 100.0 * max as f64).round() as i32;
        
        // This call typically shows the KDE overlay!
        let _ : () = self.connection.call_method(
            Some("org.kde.Solid.PowerManagement"),
            "/org/kde/Solid/PowerManagement/Actions/BrightnessControl",
            Some("org.kde.Solid.PowerManagement.Actions.BrightnessControl"),
            "setBrightness",
            &(target),
        )
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Set Error: {}", e)))?
        .body().deserialize()
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Result Error: {}", e)))?;
            
        Ok(())
    }

    fn name(&self) -> &str {
        "KDE Plasma (Native DBus)"
    }
}

pub struct KdeNightLightController {
    connection: zbus::blocking::Connection,
    inhibit_cookie: std::sync::Mutex<Option<u32>>,
}

impl KdeNightLightController {
    pub fn new() -> Result<Self, HardwareError> {
        let connection = zbus::blocking::Connection::session()
            .map_err(|e| HardwareError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            
        // Check if Night Light interface exists
        let _temp: u32 = connection.call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/NightLight",
            Some("org.kde.KWin.NightLight"),
            "currentTemperature",
            &(),
        )
        .map_err(|_| HardwareError::NotSupported)?
        .body().deserialize()
        .map_err(|_| HardwareError::NotSupported)?;
        
        Ok(Self { 
            connection,
            inhibit_cookie: std::sync::Mutex::new(None)
        })
    }
    
    pub fn set_kelvin(&self, kelvin: u32) -> Result<(), HardwareError> {
        // Hybrid Control Strategy:
        // - Day (>= 5500K): Inhibit Night Light (Forces 6500K)
        // - Night (< 5500K): Uninhibit & use Preview
        
        // Use Mutex for thread safety
        let mut cookie_opt = self.inhibit_cookie.lock().map_err(|_| HardwareError::CommandFailed("Mutex Poisoned".into()))?;
        
        if kelvin >= 5500 {
            // Target is Day/Neutral
            if cookie_opt.is_none() {
                 match self.connection.call_method(
                    Some("org.kde.KWin"),
                    "/org/kde/KWin/NightLight",
                    Some("org.kde.KWin.NightLight"),
                    "inhibit",
                    &(),
                ) {
                    Ok(msg) => {
                        if let Ok(cookie) = msg.body().deserialize::<u32>() {
                            *cookie_opt = Some(cookie);
                            info!("Running Day Mode: Inhibited Night Light (Cookie: {})", cookie);
                        }
                    },
                    Err(e) => warn!("Failed to inhibit Night Light: {}", e),
                }
            }
            // If already inhibited, do nothing (we are at 6500K)
        } else {
            // Target is Night
            if let Some(cookie) = *cookie_opt {
                // We are inhibited, must uninhibit first
                 let _ = self.connection.call_method(
                    Some("org.kde.KWin"),
                    "/org/kde/KWin/NightLight",
                    Some("org.kde.KWin.NightLight"),
                    "uninhibit",
                    &(cookie),
                );
                *cookie_opt = None;
                info!("Running Night Mode: Uninhibited Night Light");
            }
            
            // Now set preview
            let _ : () = self.connection.call_method(
                Some("org.kde.KWin"),
                "/org/kde/KWin/NightLight",
                Some("org.kde.KWin.NightLight"),
                "preview",
                &(kelvin),
            )
            .map_err(|e| HardwareError::CommandFailed(format!("NightLight DBus Error: {}", e)))?
            .body().deserialize()
            .ok()
            .unwrap_or(()); 
        }

        Ok(())
    }
    
    pub fn get_current_kelvin(&self) -> Result<u32, HardwareError> {
        // Read actual system value using D-Bus Properties interface
        // This gets what KDE Night Light is actually applying, not our target
        use zbus::zvariant::Value;
        
        let reply = self.connection.call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/NightLight",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.kde.KWin.NightLight", "currentTemperature"),
        )
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Properties.Get Error: {}", e)))?;
        
        let body = reply.body();
        let variant: Value = body.deserialize()
            .map_err(|e| HardwareError::CommandFailed(format!("Deserialize Error: {}", e)))?;
        
        match variant {
            Value::U32(temp) => Ok(temp),
            _ => Err(HardwareError::CommandFailed("Unexpected property type".into())),
        }
    }
    
    #[allow(dead_code)]
    pub fn get_kelvin(&self) -> Result<u32, HardwareError> {
        // Legacy method - calls currentTemperature as method (may not work)
        let temp: u32 = self.connection.call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/NightLight",
            Some("org.kde.KWin.NightLight"),
            "currentTemperature",
            &(),
        )
        .map_err(|e| HardwareError::CommandFailed(format!("DBus Error: {}", e)))?
        .body().deserialize()
        .map_err(|e| HardwareError::CommandFailed(format!("Parse Error: {}", e)))?;
        
        Ok(temp)
    }
    
    pub fn name(&self) -> &str {
        "KDE Night Light (Native DBus)"
    }
}



