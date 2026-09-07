//! Tauri shell. Command bodies live in the `rbl-*` crates; everything here is
//! a thin adapter so the backend stays testable without a webview.

mod commands;
mod dto;
mod error;
mod state;

pub use error::{AppError, AppResult, ErrorKind};

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

/// Loads the library off the UI thread and hands it to the state.
///
/// Read-only always: this application never opens the user's library for
/// writing during startup, and `rbl-db` refuses it while rekordbox runs.
fn spawn_library_load(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        match rbl_db::Library::open_installed_read_only() {
            Ok(db) => {
                let db_version = db.schema().db_version;
                let share_root = db.location().share_root.clone();
                match rbl_index::load(&db) {
                    Ok((library, stats)) => {
                        let load_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                        tracing::info!(
                            tracks = stats.tracks,
                            playlists = stats.playlists,
                            heap_mb = stats.heap_bytes / 1_048_576,
                            load_ms,
                            "library loaded"
                        );
                        // Writes are gated on rekordbox not running, which we
                        // re-check per transaction; the banner reflects it now.
                        let read_only = rbl_db::is_rekordbox_running();
                        app.state::<Arc<AppState>>()
                            .set_library(library, read_only, db_version, load_ms, share_root);
                        let _ = tauri::Emitter::emit(&app, "library:ready", ());
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "could not index the library");
                        let _ = tauri::Emitter::emit(&app, "library:error", e.to_string());
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "could not open the library");
                let _ = tauri::Emitter::emit(&app, "library:error", e.to_string());
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rekordbox_lite=info,rbl_db=info,rbl_index=info".into()),
        )
        .init();

    std::panic::set_hook(Box::new(|info| {
        tracing::error!(%info, "panic");
    }));

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(AppState::new()))
        .setup(|app| {
            spawn_library_load(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_summary,
            commands::playlist_tree,
            commands::open_view,
            commands::fetch_rows,
            commands::view_ids_in_range,
            commands::track_waveform,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "fatal: could not start the application");
        std::process::exit(1);
    }
}
