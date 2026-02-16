use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{warn, debug}; // info removed
use std::fs;
use std::io::Read;

pub struct ContentAnalyzer {
    last_check: Instant,
}

impl ContentAnalyzer {
    pub fn new() -> Self {
        Self {
            last_check: Instant::now().checked_sub(Duration::from_secs(5)).unwrap(),
        }
    }

    pub fn get_screen_brightness(&mut self) -> Option<f64> {
        // Limit polling to 1Hz (1000ms) because spectacle is slow (~400ms)
        if self.last_check.elapsed() < Duration::from_millis(1000) {
            return None;
        }
        self.last_check = Instant::now();

        let tmp_path = "/tmp/ab_capture.ppm";
        
        // Execute spectacle to take a background (-b) non-notifying (-n) fullscreen (-f) screenshot of monitor 0 (-m 0) to file (-o)
        // We specify monitor 0 because the daemon has no "active window" or mouse focus context.
        let output = Command::new("spectacle")
            .arg("-b")
            .arg("-n")
            .arg("-f")
            .arg("-m")
            .arg("0")
            .arg("-o")
            .arg(tmp_path)
            .output();

        match output {
            Ok(o) => {
                if !o.status.success() {
                    let err = String::from_utf8_lossy(&o.stderr);
                    debug!("Spectacle failed (Exit {}): {}", o.status.code().unwrap_or(-1), err);
                    return None;
                }
                
                // Read from file
                let mut file = match fs::File::open(tmp_path) {
                    Ok(f) => f,
                    Err(e) => {
                        debug!("Failed to open temp file: {}", e);
                        return None;
                    }
                };
                
                let mut data = Vec::new();
                if file.read_to_end(&mut data).is_err() {
                    let _ = fs::remove_file(tmp_path);
                    return None; 
                }
                
                // Remove file immediately
                let _ = fs::remove_file(tmp_path);

                if data.len() < 20 { 
                    debug!("Data too short: {}", data.len());
                    return None; 
                } 

                // Robust PPM (P6) Parser
                // Format:
                // P6 [whitespace] width [whitespace] height [whitespace] maxval [whitespace/single character] [DATA]
                // Whitespace can be space, tab, CR, LF.
                // Comments start with # and go to end of line.
                
                let mut pos; // = 0 removed
                
                // Helper to skip whitespace and comments
                let skip_whitespace_and_comments = |data: &[u8], mut p: usize| -> usize {
                    loop {
                        while p < data.len() && (data[p] as char).is_whitespace() {
                            p += 1;
                        }
                        if p < data.len() && data[p] == b'#' {
                            while p < data.len() && data[p] != b'\n' {
                                p += 1;
                            }
                        } else {
                            break;
                        }
                    }
                    p
                };
                
                // Read next number
                let read_number = |data: &[u8], mut p: usize| -> Option<(usize, usize)> {
                    p = skip_whitespace_and_comments(data, p);
                    let start = p;
                    while p < data.len() && (data[p] as char).is_ascii_digit() {
                        p += 1;
                    }
                    if start == p { return None; }
                    let s = std::str::from_utf8(&data[start..p]).ok()?;
                    let val = s.parse::<usize>().ok()?;
                    Some((val, p))
                };

                // Check Magic P6
                if data[0] != b'P' || data[1] != b'6' {
                    debug!("Invalid PPM Magic: {:?}", &data[0..2]);
                    return None;
                }
                pos = 2;

                // Read Width
                let (_width, next_pos) = match read_number(&data, pos) {
                    Some(v) => v,
                    None => { debug!("Failed to parse width"); return None; }
                };
                pos = next_pos;

                // Read Height
                let (_height, next_pos) = match read_number(&data, pos) {
                     Some(v) => v,
                     None => { debug!("Failed to parse height"); return None; }
                };
                pos = next_pos;

                // Read Maxval
                let (maxval, next_pos) = match read_number(&data, pos) {
                     Some(v) => v,
                     None => { debug!("Failed to parse maxval"); return None; }
                };
                pos = next_pos;
                
                // Skip exactly one whitespace character after maxval (usually newline)
                if pos < data.len() && (data[pos] as char).is_whitespace() {
                    pos += 1;
                }

                if pos >= data.len() {
                    debug!("No data after header");
                    return None;
                }

                let pixels = &data[pos..];
                // Stride 50 is fine for 1080p
                let stride = 50; 
                let mut total_luma = 0.0;
                let mut count = 0;

                // RGB is 3 bytes
                for i in (0..pixels.len()).step_by(3 * stride) {
                    if i + 2 >= pixels.len() { break; }
                    let r = pixels[i] as f64;
                    let g = pixels[i+1] as f64;
                    let b = pixels[i+2] as f64;
                    
                    // Normalize to 0-1 based on maxval
                    let r_n = r / maxval as f64;
                    let g_n = g / maxval as f64;
                    let b_n = b / maxval as f64;
                    
                    // Rec. 601 luma
                    let luma = 0.299 * r_n + 0.587 * g_n + 0.114 * b_n;
                    total_luma += luma;
                    count += 1;
                }
                
                if count == 0 { return None; }
                
                let avg_luma = total_luma / count as f64;
                
                Some(avg_luma)
            },
            Err(e) => {
                warn!("Failed to execute spectacle: {}", e);
                None
            }
        }
    }
}
