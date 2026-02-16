use chrono::{TimeZone, Utc, Duration};
use core::context::ContextManager;
use core::config::LocationConfig;

mod config_stub {
    pub struct LocationConfig {
        pub latitude: Option<f64>,
        pub longitude: Option<f64>,
    }
}

fn main() {
    let config = core::config::LocationConfig {
        latitude: Some(41.0082), // Istanbul
        longitude: Some(28.9784),
    };
    
    let ctx = ContextManager::new(&config, "07:00");
    
    // Test current time (approx 23:17 UTC+3 -> 20:17 UTC)
    let now = Utc::now();
    println!("Current Real Time (UTC): {}", now);
    
    // Simulate times
    let hours_to_test = vec![0, 3, 6, 7, 8, 12, 17, 18, 19, 20, 21, 22, 23];
    
    println!("{:<10} | {:<10} | {:<10} | {:<10}", "Hour (Loc)", "Mins", "Kelvin", "Brightness");
    println!("---------------------------------------------------------");
    
    for h in hours_to_test {
        // Construct a UTC time that corresponds to 'h' local time (UTC+3)
        // h_local = h_utc + 3  => h_utc = h_local - 3
        let h_utc = (h + 24 - 3) % 24;
        
        // This is a rough approximation for today
        let today_utc = Utc::now().date_naive().and_hms_opt(h_utc as u32, 17, 0).unwrap().and_utc();
        
        let k = ctx.get_kelvin_target(today_utc);
        let b = ctx.get_circadian_target(today_utc);
        
        // Calculate minutes from midnight local
        let mins = h * 60 + 17;
        
        println!("{:02}:17      | {:<10} | {:<10} | {:.1}", h, mins, k, b);
    }
    
    // Debugging current sunset
    let sun_times = ctx.calculate_sun_times(now);
    println!("\nDebug Sun Times (UTC):");
    println!("Sunrise: {}", sun_times.sunrise);
    println!("Sunset:  {}", sun_times.sunset);
    
    // Check specific user time 23:17 (Local) -> 20:17 UTC
    // NOTE: If date is different, sun times differ.
}
