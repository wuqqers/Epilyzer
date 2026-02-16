#[cfg(test)]
mod tests {
    use super::*;
    use crate::epilepsy::{EpilepsyGuard, SafetyMode};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_initialization() {
        let guard = EpilepsyGuard::new(50.0);
        assert_eq!(guard.current_brightness, 50.0);
        assert_eq!(guard.mode, SafetyMode::Automatic);
    }

    #[test]
    fn test_frequency_limit() {
        let guard = EpilepsyGuard::new(50.0);
        // Initially blocked because last_change_time is now and MIN_SAFE_INTERVAL is ~333ms
        assert!(!guard.can_update()); 
        
        thread::sleep(Duration::from_millis(350));
        assert!(guard.can_update()); // Should be allowed after wait
    }

    #[test]
    fn test_step_limit() {
        let mut guard = EpilepsyGuard::new(50.0);
        
        // Try to jump to 100
        let next = guard.calculate_next_step(100.0);
        
        // Should only increase by MAX_DELTA_PER_STEP (2.0)
        assert!((next - 52.0).abs() < 0.01);
        assert_eq!(guard.current_brightness, next);
    }

    #[test]
    fn test_transition_duration() {
        let mut guard = EpilepsyGuard::new(20.0);
        
        // Request move to 80
        // Logic uses fixed transition_duration_ms (default 750ms)
        guard.request_transition(80.0);
        
        if let Some(trans) = guard.transition {
            assert_eq!(trans.duration.as_millis(), 750);
        } else {
            panic!("Transition not started");
        }
    }
    
    #[test]
    fn test_emergency_stop() {
        let mut guard = EpilepsyGuard::new(50.0);
        guard.mode = SafetyMode::EmergencyStop;
        
        let next = guard.calculate_next_step(100.0);
        assert_eq!(next, 50.0); // No change allowed
        
        guard.request_transition(80.0);
        assert!(guard.transition.is_none()); // No transition allowed
    }

    #[test]
    fn test_transition_update() {
        let mut guard = EpilepsyGuard::new(50.0);
        guard.request_transition(80.0);
        
        // Check state
        assert!(guard.transition.is_some());
        assert_eq!(guard.transition.as_ref().unwrap().target_brightness, 80.0);
        
        // Interrupt with new target
        guard.request_transition(20.0);
        assert_eq!(guard.transition.as_ref().unwrap().target_brightness, 20.0);
        
        // Interrupt with instant
        guard.force_instant_transition(100.0);
        assert_eq!(guard.transition.as_ref().unwrap().target_brightness, 100.0);
        assert_eq!(guard.transition.as_ref().unwrap().duration.as_millis(), 200);
    }
}
