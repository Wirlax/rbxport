//! Tauri shell. Command bodies live in the `rbl-*` crates; everything here is
//! a thin adapter so the backend stays testable without a webview.

mod error;

pub use error::{AppError, AppResult, ErrorKind};

use rbl_core::FourCc;

/// Placeholder until `rbl-db` lands in Milestone 1.
#[tauri::command]
fn app_info() -> AppResult<serde_json::Value> {
    error::run_command("app_info", || {
        Ok(serde_json::json!({
            "name": "rekordbox-lite",
            "version": env!("CARGO_PKG_VERSION"),
            "anlzProbeTag": FourCc::new(b"PQTZ").as_str(),
        }))
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        tracing::error!(%info, "panic");
    }));

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "fatal: could not start the application");
        std::process::exit(1);
    }
}
