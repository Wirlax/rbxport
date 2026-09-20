//! Compare seeked PCM with sequential decoding, without opening an audio device.
// Diagnostic only: reads the file and compares PCM; no device or library writes.
#![allow(
    clippy::pedantic,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used
)]
#[allow(dead_code)]
#[path = "../src/decode.rs"]
mod decode;
use rbl_deck::{DeckError, Result};
fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("audio path"));
    for rate in [44100, 48000] {
        let mut linear = decode::Streamer::open(&path, rate).unwrap();
        let mut reference = vec![0f32; rate as usize * 35 * 2];
        let n = linear.fill(&mut reference).unwrap();
        println!(
            "rate {rate}, reference {n} frames, total {}",
            linear.total_frames()
        );
        for seconds in [0.0, 10.0, 24.4, 25.0, 26.4] {
            let at = (seconds * rate as f64) as usize;
            if at + rate as usize * 2 >= n {
                continue;
            }
            let mut seek = decode::Streamer::open(&path, rate).unwrap();
            let started = std::time::Instant::now();
            seek.seek(at as u64).unwrap();
            let seek_ms = started.elapsed().as_secs_f64() * 1000.0;
            let mut samples = vec![0f32; rate as usize * 3 * 2];
            seek.fill(&mut samples).unwrap();
            let start = rate as usize;
            let count = 4096;
            let mut best = (f64::INFINITY, 0i32);
            for shift in -2048i32..=2048 {
                let mut error = 0f64;
                for i in (start..start + count).step_by(16) {
                    let a = samples[i * 2];
                    let j = (at + i) as i64 + shift as i64;
                    let b = reference[j as usize * 2];
                    error += (a as f64 - b as f64).powi(2);
                }
                if error < best.0 {
                    best = (error, shift);
                }
            }
            println!("seek {seconds} ({seek_ms:.2} ms): best reference offset {} frames ({:.3} ms), SSE {:.6}",best.1,best.1 as f64*1000.0/rate as f64,best.0);
        }
    }
}
