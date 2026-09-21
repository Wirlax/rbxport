//! Process-entry to frontend paint opportunities, including webview startup.
use std::sync::{atomic::{AtomicU8, Ordering}, OnceLock};
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static REPORTED: AtomicU8 = AtomicU8::new(0);

pub fn begin() { let _ = START.set(Instant::now()); }

#[tauri::command]
pub fn startup_milestone(window: tauri::WebviewWindow, phase: String) {
    if window.label() != "main" { return; }
    let flag = match phase.as_str() {
        "shell-painted" => 1,
        "first-rows-painted" => 2,
        _ => return,
    };
    if REPORTED.fetch_or(flag, Ordering::Relaxed) & flag != 0 { return; }
    if let Some(start) = START.get() {
        tracing::info!(phase, elapsed_ms = start.elapsed().as_millis(), "startup milestone");
    }
}
