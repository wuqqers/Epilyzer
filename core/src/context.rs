use chrono::{DateTime, Utc, Timelike, NaiveTime, Datelike};
use tracing::info;
use crate::config::LocationConfig;

pub struct ContextManager {
    _lat: f64,
    lon: f64,
    wake_time: chrono::NaiveTime,
}

impl ContextManager {
    pub fn new(config: &LocationConfig, wake_time_str: &str) -> Self {
        let lat = config.latitude.unwrap_or(41.0082);
        let lon = config.longitude.unwrap_or(28.9784);
        
        let wake_time = chrono::NaiveTime::parse_from_str(wake_time_str, "%H:%M")
            .unwrap_or_else(|_| chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap());

        info!("Context initialized at Lat: {}, Lon: {}, Wake: {}", lat, lon, wake_time);
        
        Self { _lat: lat, lon, wake_time }
    }

    pub fn get_wake_time(&self) -> (u8, u8) {
        (self.wake_time.hour() as u8, self.wake_time.minute() as u8)
    }

    pub fn set_wake_time(&mut self, hour: u8, minute: u8) {
        if let Some(new_time) = NaiveTime::from_hms_opt(hour as u32, minute as u32, 0) {
             self.wake_time = new_time;
        }
    }

    // NOAA Solar Position Algorithm (Simplified)
    // Returns solar elevation in degrees (positive = day, negative = night)
    pub fn calculate_solar_elevation(&self, date: DateTime<Utc>) -> f64 {
        use std::f64::consts::PI;
        
        // 1. Day of year (1-366)
        let doy = date.ordinal() as f64;
        let hour = date.hour() as f64 + date.minute() as f64 / 60.0 + date.second() as f64 / 3600.0;
        
        // 2. Fractional year (radians)
        let gamma = (2.0 * PI / 365.0) * (doy - 1.0 + (hour - 12.0) / 24.0);
        
        // 3. Equation of time (minutes)
        let eq_time = 229.18 * (0.000075 + 0.001868 * gamma.cos() - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos() - 0.040849 * (2.0 * gamma).sin());
            
        // 4. Solar declination (radians)
        let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
            - 0.006758 * (2.0 * gamma).cos() + 0.000907 * (2.0 * gamma).sin()
            - 0.002697 * (3.0 * gamma).cos() + 0.00148 * (3.0 * gamma).sin();
            
        // 5. True solar time (minutes)
        let time_offset = eq_time + 4.0 * self.lon; // 4 mins per degree longitude
        let tst = hour * 60.0 + time_offset;
        
        // 6. Solar hour angle (degrees)
        let ha = (tst / 4.0) - 180.0;
        let ha_rad = ha.to_radians();
        
        // 7. Solar Zenith Angle (radians)
        // cos(phi) = sin(lat)*sin(decl) + cos(lat)*cos(decl)*cos(ha)
        let lat_rad = self._lat.to_radians();
        let cos_zenith = lat_rad.sin() * decl.sin() + lat_rad.cos() * decl.cos() * ha_rad.cos();
        let zenith_rad = cos_zenith.acos();
        
        // 8. Solar Elevation (degrees) = 90 - Zenith
        let elevation = 90.0 - zenith_rad.to_degrees();
        
        elevation
    }
    
    pub fn get_circadian_target(&self, now: DateTime<Utc>) -> f64 {
        let elevation = self.calculate_solar_elevation(now);
        
        // Check wake time override (simple check)
        let now_local = now.hour() + 3; // Approx
        if now_local < self.wake_time.hour() {
             return 10.0; // Sleep brightness
        }


        
        // Calculate target based on progress/elevation logic above (which returned early)
        // Wait, my previous replacement had early returns!
        // I need to refactor to not return early if I want to log at the end, 
        // OR simple add logging before each return.
        
        // Let's rewrite get_circadian_target to be cleaner and log.
        
        let target_b = if elevation > 6.0 {
            let day_progress = ((elevation - 6.0) / 40.0).clamp(0.0, 1.0);
            50.0 + (50.0 * day_progress)
        } else if elevation > -6.0 {
            let t_progress = (6.0 - elevation) / 12.0;
            50.0 - (20.0 * t_progress)
        } else if elevation > -12.0 {
            let t_progress = (-6.0 - elevation) / 6.0;
            30.0 - (10.0 * t_progress)
        } else {
            20.0
        };
        
        info!("Solar Algo: Elevation {:.2}°, Target Brightness {:.1}%", elevation, target_b);
        
        target_b
    }
}
