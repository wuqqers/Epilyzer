use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

pub const MIN_TRANSITION_TIME_SEC: f64 = 2.0; 
pub const MAX_CHANGE_FREQUENCY_HZ: f64 = 3.0;
pub const MIN_SAFE_INTERVAL_MS: u128 = (1000.0 / MAX_CHANGE_FREQUENCY_HZ) as u128;
pub const MAX_DELTA_PER_STEP: f64 = 2.0;
pub const RED_FLASH_THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyMode {
    Automatic,
    EmergencyStop,
}

#[derive(Debug, Clone)]
pub struct TransitionState {
    pub current_brightness: f64,
    pub target_brightness: f64,
    pub start_time: Instant,
    pub duration: Duration,
    pub initial_brightness: f64,
}

pub struct EpilepsyGuard {
    pub mode: SafetyMode,
    pub last_change_time: Instant,
    pub current_brightness: f64,
    pub transition: Option<TransitionState>,
    pub last_user_override: Option<Instant>,
    pub is_locked: bool,
    pub transition_duration_ms: u64,
}

impl EpilepsyGuard {
    pub fn new(initial_brightness: f64) -> Self {
        Self {
            mode: SafetyMode::Automatic,
            last_change_time: Instant::now(),
            current_brightness: initial_brightness,
            transition: None,
            last_user_override: None,
            is_locked: false,
            transition_duration_ms: 750, // Default
        }
    }

    pub fn set_transition_duration(&mut self, ms: u64) {
        // Clamp to safe range: 300ms (still fast) to 2000ms (very slow)
        self.transition_duration_ms = ms.clamp(300, 2000);
        info!("Transition duration set to {}ms", self.transition_duration_ms);
    }

    pub fn set_user_override(&mut self) {
        self.last_user_override = Some(Instant::now());
    }

    pub fn get_safety_cap(&self) -> f64 {
        match self.mode {
            SafetyMode::EmergencyStop => 0.0,
            _ => 100.0,
        }
    }

    pub fn is_in_grace_period(&self, duration: Duration) -> bool {
        if let Some(last) = self.last_user_override {
            last.elapsed() < duration
        } else {
            false
        }
    }

    pub fn can_update(&self) -> bool {
        if self.mode == SafetyMode::EmergencyStop {
            return false;
        }
        let elapsed = self.last_change_time.elapsed().as_millis();
        elapsed >= MIN_SAFE_INTERVAL_MS
    }

    fn clamp_safe(val: f64) -> f64 {
        val.clamp(5.0, 100.0)
    }

    pub fn calculate_next_step(&mut self, target: f64) -> f64 {
         if self.mode == SafetyMode::EmergencyStop {
            return self.current_brightness;
        }

        let cap = self.get_safety_cap();
        let target = Self::clamp_safe(target.min(cap));

        if (self.current_brightness - target).abs() < 0.1 {
             return self.current_brightness;
        }

        let max_change = MAX_DELTA_PER_STEP;
        
        let diff = target - self.current_brightness;
        let step = diff.clamp(-max_change, max_change);
        
        let new_brightness = Self::clamp_safe(self.current_brightness + step);
        
        self.current_brightness = new_brightness;
        self.last_change_time = Instant::now();

        new_brightness
    }

    pub fn request_transition(&mut self, target: f64) {
         if self.mode == SafetyMode::EmergencyStop {
            warn!("Transition requested during EMERGENCY STOP - Ignored");
            return;
        }

        let cap = self.get_safety_cap();
        let target = target.min(cap);

        // If we are already transitioning to this target, do nothing
        if let Some(ref trans) = self.transition {
            if (trans.target_brightness - target).abs() < 0.1 {
                return;
            }
        }

        let distance = (target - self.current_brightness).abs();
        if distance < 0.1 {
            return;
        }

        // Use configurable transition duration (epilepsy-safe, WCAG compliant)
        self.transition = Some(TransitionState {
            current_brightness: self.current_brightness,
            initial_brightness: self.current_brightness,
            target_brightness: target,
            start_time: Instant::now(),
            duration: Duration::from_millis(self.transition_duration_ms),
        });
        
        info!("Transition started: {:.1} -> {:.1} ({}ms)", 
              self.current_brightness, target, self.transition_duration_ms);
    }

    pub fn force_instant_transition(&mut self, target: f64) {
        let cap = self.get_safety_cap();
        let target = target.min(cap);
        
        // If we are already transitioning to this target, do nothing
        if let Some(ref trans) = self.transition {
            if (trans.target_brightness - target).abs() < 0.1 {
                return;
            }
        }
        
        let distance = (target - self.current_brightness).abs();
        if distance < 0.1 {
            return;
        }

        // Force fast transition (200ms) for safety/responsiveness
        // This overrides the user's "smooth" preference because it's usually for flashbang protection
        self.transition = Some(TransitionState {
            current_brightness: self.current_brightness,
            initial_brightness: self.current_brightness,
            target_brightness: target,
            start_time: Instant::now(),
            duration: Duration::from_millis(200),
        });
        info!("Fast transition started: {:.1} -> {:.1} (200ms)", self.current_brightness, target);
    }

    pub fn tick_transition(&mut self) -> Option<f64> {
        if self.mode == SafetyMode::EmergencyStop {
             self.transition = None;
             return None;
        }

        if let Some(ref trans) = self.transition {
            let elapsed = trans.start_time.elapsed().as_secs_f64();
            let total_dur = trans.duration.as_secs_f64();
            
            if elapsed >= total_dur {
                let final_val = trans.target_brightness;
                self.current_brightness = final_val;
                self.transition = None;
                return Some(final_val);
            }

            let t = elapsed / total_dur;
            let eased_t = Self::ease_in_out(t);
            
            let new_val = trans.initial_brightness + (trans.target_brightness - trans.initial_brightness) * eased_t;
            
            self.current_brightness = new_val;
            self.last_change_time = Instant::now();
            return Some(new_val);
        }
        None
    }
    
    pub fn ease_in_out(t: f64) -> f64 {
        -( (std::f64::consts::PI * t).cos() - 1.0) / 2.0
    }
}
