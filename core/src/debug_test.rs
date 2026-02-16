#[cfg(test)]
mod tests {
    use crate::context::ContextManager;
    use crate::config::LocationConfig;
    use chrono::{TimeZone, Utc, Timelike};

    #[test]
    fn test_circadian_outputs() {
        let config = LocationConfig {
            latitude: Some(41.0082),
            longitude: Some(28.9784),
            method: "dummy".to_string(),
            timezone: "Europe/Istanbul".to_string(),
        };
        
        let ctx = ContextManager::new(&config, "07:00");
        
        println!("{:<10} | {:<10} | {:<10}", "Hour (Loc)", "Mins", "Brightness");
        println!("-------------------------------------------");
        
        let hours_to_test = vec![0, 3, 6, 7, 8, 12, 17, 18, 19, 20, 21, 22, 23];
        
        for h in hours_to_test {
            // h_local = h_utc + 3
            let h_utc = (h + 24 - 3) % 24;
            let date = Utc::now().date_naive().and_hms_opt(h_utc as u32, 17, 0).unwrap().and_utc();
            
            let b = ctx.get_circadian_target(date);
            let mins = h * 60 + 17;
            
            println!("{:02}:17      | {:<10} | {:.1}", h, mins, b);
        }
    }
}
