use anyhow::{Context, Result};
use clap::Parser;
use core::config::Config;
use core::epilepsy::EpilepsyGuard;
use core::hardware::{BrightnessController, BacklightController, DdcUtilController, DummyController};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, error, warn, Level};
use tracing_subscriber::FmtSubscriber;
use std::fs;
use std::process::Command;

mod logging;
// mod ml; // Removed as unused
mod state;
mod content;

use crate::state::StateManager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "/etc/auto-brightness/config.toml")]
    config: PathBuf,

    #[arg(long)]
    dry_run: bool,
}

fn is_on_battery() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("BAT") {
                let status_path = path.join("status");
                if let Ok(status) = fs::read_to_string(status_path) {
                    if status.trim() == "Discharging" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let args = Args::parse();
    info!("Starting Auto-Brightness Daemon...");

    let config = if args.config.exists() {
        Config::load_from_file(&args.config).context("Failed to load config")?
    } else {
        warn!("Config file not found at {:?}, using defaults", args.config);
        Config::default()
    };

    let mut controller: Box<dyn BrightnessController + Send> = if args.dry_run {
         info!("Using Dummy Controller (Dry Run)");
         Box::new(DummyController::new())
    } else {
        // High Refresh Rate Optimization:
        // Always prefer BacklightController (sysfs) if available because it is:
        // 1. Silent (No OSD overlay spam)
        // 2. Fast (Direct file write vs DBus RTT)
        // 3. Essential for 60Hz/120Hz updates
        
        let mut tried_backlight = false;
        let mut best_controller: Option<Box<dyn BrightnessController + Send>> = None;

        if config.brightness.method == "auto" || config.brightness.method == "backlight" {
             if let Ok(c) = BacklightController::auto() {
                 info!("✅ Using Backlight Controller (sysfs) - Optimized for High Refresh Rate");
                 best_controller = Some(Box::new(c));
                 tried_backlight = true;
             }
        }

        if best_controller.is_none() {
            let is_kde = std::env::var("KDE_FULL_SESSION").map(|v| v == "true").unwrap_or(false) 
                         || std::env::var("DESKTOP_SESSION").map(|v| v.contains("plasma")).unwrap_or(false);
                         
            if is_kde {
                info!("Detected KDE Plasma Session.");
                if !tried_backlight {
                    // Try backlight again if we skipped it due to config but now are falling back? 
                    // No, adhere to config. But if config was 'auto' (default), we already tried.
                }

                if let Ok(c) = core::hardware::KdeBrightnessController::new() {
                    info!("✅ Using Native KDE Controller (DBus) - Warning: May trigger OSD and latency");
                    best_controller = Some(Box::new(c));
                }
            }
        }
        
        // Final Fallback
        if best_controller.is_none() {
            match config.brightness.method.as_str() {
                "ddcutil" => Box::new(DdcUtilController::new(1)),
                _ => {
                    // Try backlight one last time if we haven't
                    if !tried_backlight {
                         match BacklightController::auto() {
                            Ok(c) => Box::new(c),
                            Err(_) => Box::new(DummyController::new())
                        }
                    } else {
                        Box::new(DummyController::new())
                    }
                }
            }
        } else {
            best_controller.unwrap()
        }
    };

    let state_manager = Arc::new(Mutex::new(StateManager::new()));
    let (initial_b, stored_wake, stored_trans, stored_flashbang) = {
        let sm = state_manager.lock().unwrap();
        let state = sm.load();
        (state.brightness, state.wake_time, state.transition_duration_ms, state.flashbang_protection)
    };
    
    let safe_initial = if initial_b < 5.0 { 15.0 } else { initial_b };
    info!("Initial brightness (Persisted): {:.1}%", safe_initial);
    if let Err(e) = controller.set_brightness(safe_initial) {
        error!("Failed to set initial brightness: {}", e);
    } else {
        info!("✅ Applied initial brightness: {:.1}%", safe_initial);
    }



    let mut guard = EpilepsyGuard::new(safe_initial);
    guard.set_transition_duration(stored_trans);
    let guard = Arc::new(Mutex::new(guard));
    
    let flashbang_enabled = Arc::new(Mutex::new(stored_flashbang));
    let fb_enabled_ref = flashbang_enabled.clone();
    let mut context = core::context::ContextManager::new(&config.location, &config.general.wake_time);
    

    
    if let Some((h, m)) = stored_wake {
        info!("Restoring persisted wake time: {:02}:{:02}", h, m);
        context.set_wake_time(h, m);
    }
    
    let context = Arc::new(Mutex::new(context));
    
    // We removed ML entirely from usage, but kept the struct to avoid errors
    // let mut predictor = crate::ml::Predictor::new();

    let socket_path = "/tmp/auto_brightness.sock";
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path).ok();
    }
    let listener = tokio::net::UnixListener::bind(socket_path).context("Failed to bind IPC socket")?;

    info!("Daemon running. Listening on {}", socket_path);

    // Fast Loop: UI Updates, Flashbang, Transitions (100ms)

 


    let last_heartbeat = Arc::new(Mutex::new(Instant::now()));
    let weather_modifier = Arc::new(Mutex::new(1.0));
    let weather_mod_ref = weather_modifier.clone();
    
    tokio::spawn(async move {
        loop {
            if let Ok(output) = Command::new("curl").arg("-s").arg("wttr.in/?format=%C").output() {
                let condition = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
                let factor: f64 = if condition.contains("sun") || condition.contains("clear") {
                    1.0
                } else if condition.contains("partly") {
                    0.9
                } else if condition.contains("cloud") || condition.contains("overcast") || condition.contains("mist") || condition.contains("fog") {
                    0.8
                } else if condition.contains("rain") || condition.contains("snow") || condition.contains("drizzle") || condition.contains("thunder") {
                    0.7
                } else {
                    1.0 
                };
                {
                    let mut m = weather_mod_ref.lock().unwrap();
                    if (*m - factor).abs() > 0.01 {
                         info!("Weather Sync: '{}' -> Scaling brightness by {:.2}", condition, factor);
                         *m = factor;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1800)).await;
        }
    });

    // ---------------------------------------------------------
    // ASYNC CONTENT ANALYSIS TASK
    // ---------------------------------------------------------
    // Decouple blocking spectacle calls from the main loop to allow 120Hz smooth transitions.
    let luma_shared = Arc::new(Mutex::new(None::<f64>));
    let luma_writer = luma_shared.clone();
    
    // We only need one analyzer instance
    tokio::spawn(async move {
        // Delay start slightly to let daemon settle
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        let mut content_analyzer = crate::content::ContentAnalyzer::new();
        loop {
            if let Some(val) = content_analyzer.get_screen_brightness() {
                *luma_writer.lock().unwrap() = Some(val);
            }
            // 100ms interval for content checks is sufficient (10fps for content changes)
            // The main loop will interpolate smoothly at 120Hz.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let mut content_multiplier = 1.0;
    
    // ---------------------------------------------------------
    // HIGH FREQUENCY MAIN LOOP (125Hz / 8ms)
    // ---------------------------------------------------------
    info!("🚀 Starting High-Frequency Loop (8ms / 125Hz) for smooth transitions");
    
    let mut tick_count: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(8)); 

    loop {
        tokio::select! {
            _ = interval.tick() => {
                 tick_count += 1;
                 
                 // 1. Check for new content analysis result (Non-blocking)
                 let current_luma = { *luma_shared.lock().unwrap() };
                 let is_fb_enabled = { *fb_enabled_ref.lock().unwrap() }; // Use cloning ref
                 
                 // User request: "kısmadı oysa %10'a falan çekmeli"
                 if let Some(luma) = current_luma {
                     if is_fb_enabled {
                         // Aggressive curve: Start dimming at 0.5 (was 0.7)
                         if luma > 0.5 {
                             // excess goes from 0.0 to 1.0 (at luma 1.0, excess = 0.5 / 0.5 = 1.0)
                             let excess = (luma - 0.5) / 0.5;
                             
                             // Target multiplier: 
                             // At luma 1.0 -> excess 1.0 -> target_mult = 1.0 - (1.0 * 0.95) = 0.05 (5% brightness)
                             let target_mult = 1.0 - (excess * 0.95); 
                             
                             if target_mult < content_multiplier {
                                 // Fast drop (Flashbang protection needs to be instant)
                                 // Now: Instant application to minimize eye pain
                                 content_multiplier = target_mult;
                             } else {
                                 // Recovery: Let EpilepsyGuard handle smoothing (configurable duration)
                                 // Prevent oscillation: Do not recover beyond the current target_mult!
                                 // If target_mult is 0.05 (white screen), we stay at 0.05.
                                 // 125Hz adjustment: 0.2 per tick at 10Hz was 2.0/sec.
                                 // At 125Hz, we want similar or faster instant recovery for calculation.
                                 // 0.02 * 125 = 2.5/sec. Let's use 0.05 to be sure.
                                 content_multiplier = (content_multiplier + 0.05).min(target_mult);
                             }
                         } else {
                             // Normal content, recover
                             content_multiplier = (content_multiplier + 0.05).min(1.0);
                         }
                     } else {
                         // Flashbang protection disabled by user
                         content_multiplier = 1.0;
                     }
                 } else {
                      // No luma data yet
                 }
                 
                 // 2. Main Autopilot Logic
                 // Was: tick_count % 10 (Every 1s at 10Hz)
                 // Now: tick_count % 125 (Every 1s at 125Hz)
                 if tick_count % 125 == 0 {
                    let mut g = guard.lock().unwrap();
                    if !g.is_locked && g.mode == core::epilepsy::SafetyMode::Automatic {
                         if !g.is_in_grace_period(Duration::from_secs(1800)) {
                             let now = chrono::Utc::now();
                             

                             
                             // B. Calculate Brightness Target
                             let ctx = context.lock().unwrap();
                             let mut target = ctx.get_circadian_target(now);
                             
                             let w_factor = { *weather_modifier.lock().unwrap() };
                             if w_factor < 0.99 { target *= w_factor; }
                             if content_multiplier < 0.99 { target *= content_multiplier; }
                             if is_on_battery() { target *= 0.8; }
                             
                             // C. Smart Transition Logic (Epilepsy Friendly)
                             let diff = (g.current_brightness - target).abs();
                             let is_dimming_for_safety = target < (g.current_brightness - 1.0) && content_multiplier < 0.99;
                             
                             // Rule 1: Safety First. If we need to dim due to Flashbang, do it NOW and FAST.
                             if is_dimming_for_safety {
                                 // Use instant transition (200ms) for flashbangs
                                 g.force_instant_transition(target);
                             } 
                             // Rule 2: Circadian Stability. Only change if significant drift or long time.
                             // Don't change every 2-3 mins for 1% diff.
                             else if diff > 5.0 {
                                 // Significant change (e.g. sunset started), apply.
                                 g.request_transition(target);
                             }
                             else if diff > 1.0 && tick_count % 75000 == 0 {
                                 // Very slow drift check (Every 10 mins = 75000 ticks at 125Hz)
                                 // Allow small adjustments only rarely.
                                 g.request_transition(target);
                             }
                             
                             // D. Logging (Every 5s = 625 ticks)
                             if tick_count % 625 == 0 {
                                 // let kelvin = ctx.get_kelvin_target(now);
                                 // Log detailed stats only if verbose or changes happening
                                 // info!("🔍 STATS | Target: {:.1}% | K: {} | W:x{:.2} | FB:x{:.2}", target, kelvin, w_factor, content_multiplier);
                             }
                         }
                    }
                 }

                 // 3. Hardware Tick (Smooth Transitions)
                 {
                    let mut g = guard.lock().unwrap();
                    if let Some(new_val) = g.tick_transition() {
                          if let Err(e) = controller.set_brightness(new_val) {
                              error!("HW Error: {}", e);
                          } else {
                              // Persist every 5 seconds during transition (625 ticks at 125Hz)
                              if tick_count % 625 == 0 {
                                  let ctx = context.lock().unwrap();
                                  let wt = ctx.get_wake_time();
                                  drop(ctx);
                                  let td = g.transition_duration_ms;
                                  let fb = *fb_enabled_ref.lock().unwrap();
                                  state_manager.lock().unwrap().save(new_val, Some(wt), td, fb);
                              }
                          }
                    }
                 }
            }



            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let guard_ref = guard.clone();
                        let state_ref = state_manager.clone();
                        let hb_ref = last_heartbeat.clone();
                        let ctx_ref = context.clone();
                        let weather_ref = weather_modifier.clone();
                        let fb_ref = flashbang_enabled.clone();
                        
                        *hb_ref.lock().unwrap() = Instant::now();
                        
                        tokio::spawn(async move {
                            handle_connection(stream, guard_ref, state_ref, hb_ref, ctx_ref, weather_ref, fb_ref).await;
                        });
                    }
                    Err(e) => error!("IPC Accept Error: {}", e),
                }
            }
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream, 
    guard: Arc<Mutex<EpilepsyGuard>>, 
    state_manager: Arc<Mutex<StateManager>>,
    heartbeat: Arc<Mutex<Instant>>,
    context: Arc<Mutex<core::context::ContextManager>>,
    weather_modifier: Arc<Mutex<f64>>,
    flashbang_enabled: Arc<Mutex<bool>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use core::ipc::{IpcCommand, IpcResponse};
    use crate::logging::DataLogger;

    let logger = DataLogger::new();
    let mut buf = [0; 1024];
    
    match stream.read(&mut buf).await {
        Ok(n) if n > 0 => {
             *heartbeat.lock().unwrap() = Instant::now();
             
             if let Ok(cmd) = serde_json::from_slice::<IpcCommand>(&buf[..n]) {
                 if !matches!(cmd, IpcCommand::GetInfo | IpcCommand::Heartbeat) {
                    info!("Received command: {:?}", cmd);
                 }
                 
                 let response = {
                     let mut g = guard.lock().unwrap();
                     match cmd {
                         IpcCommand::SetBrightness(val) => {
                             g.set_user_override(); 
                             g.request_transition(val);
                             logger.log("override", val, "Automatic").ok();
                             // Persist
                             {
                                 let ctx = context.lock().unwrap();
                                 let wt = ctx.get_wake_time();
                                 let td = g.transition_duration_ms;
                                 let fb = *flashbang_enabled.lock().unwrap();
                                 state_manager.lock().unwrap().save(val, Some(wt), td, fb);
                             }
                             IpcResponse::Ok
                         },
                         IpcCommand::SetWakeTime(h, m) => {
                             info!("Updating Wake Time to {:02}:{:02}", h, m);
                             {
                                 let mut ctx = context.lock().unwrap();
                                 ctx.set_wake_time(h, m);
                             }
                             {
                                 let wt = Some((h, m));
                                 let b = g.current_brightness;
                                 let td = g.transition_duration_ms;
                                 let fb = *flashbang_enabled.lock().unwrap();
                                 state_manager.lock().unwrap().save(b, wt, td, fb);
                             }
                             IpcResponse::Ok
                         },
                         IpcCommand::SetTransitionDuration(ms) => {
                             g.set_transition_duration(ms);
                             // Persist
                             {
                                  let ctx = context.lock().unwrap();
                                  let wt = ctx.get_wake_time();
                                  let b = g.current_brightness;
                                  let fb = *flashbang_enabled.lock().unwrap();
                                  state_manager.lock().unwrap().save(b, Some(wt), ms, fb);
                             }
                             IpcResponse::Ok
                         },
                         IpcCommand::SetFlashbangProtection(enabled) => {
                             *flashbang_enabled.lock().unwrap() = enabled;
                             info!("Flashbang Protection set to: {}", enabled);
                             // Persist
                             {
                                  let ctx = context.lock().unwrap();
                                  let wt = ctx.get_wake_time();
                                  let b = g.current_brightness;
                                  let td = g.transition_duration_ms;
                                  state_manager.lock().unwrap().save(b, Some(wt), td, enabled);
                             }
                             IpcResponse::Ok
                         },
                         IpcCommand::Freeze(_) => {
                               g.mode = core::epilepsy::SafetyMode::EmergencyStop;
                               warn!("EMERGENCY STOP ACTIVATED");
                               logger.log("freeze", g.current_brightness, "EMERGENCY_STOP").ok();
                               IpcResponse::Ok
                         },
                         IpcCommand::ResetAuto => {
                               info!("User requested Auto-Reset (Kontrol Et)");
                               g.last_user_override = None;
                               
                               let now = chrono::Utc::now();
                               let ctx = context.lock().unwrap();
                               

                               let mut target = ctx.get_circadian_target(now);
                               let w_factor = { *weather_modifier.lock().unwrap() };
                               if w_factor < 0.99 { target *= w_factor; }
                               
                               g.force_instant_transition(target);
                               IpcResponse::Ok
                         },
                          IpcCommand::GetInfo | IpcCommand::Heartbeat => {
                               let (h, m) = context.lock().unwrap().get_wake_time();
                               let fb = *flashbang_enabled.lock().unwrap();
                               
                                IpcResponse::Status {
                                   brightness: g.current_brightness,
                                   location: "Automatic".to_string(),
                                   wake_time: format!("{:02}:{:02}", h, m),
                                   transition_duration_ms: g.transition_duration_ms,
                                   flashbang_protection: fb,
                               }
                           }
                      }
                  };
                  
                  let resp_bytes = serde_json::to_vec(&response).unwrap();
                  stream.write_all(&resp_bytes).await.ok();
             }
        }
        _ => {}
    }
}
