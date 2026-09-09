//! Tauri shell. Command bodies live in the `rbl-*` crates; everything here is
//! a thin adapter so the backend stays testable without a webview.

mod windowfit;
mod commands;
mod diagnostics;
mod link;
pub mod menu;
mod player;
mod protocol;
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
/// Where the library snapshot lives.
///
/// Under the app's own data directory, not the library's: it is derived, it is
/// ours, and nothing outside this app should ever find it next to rekordbox's
/// files.
fn cache_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager as _;
    Some(app.path().app_cache_dir().ok()?.join("library.snapshot"))
}

/// The schema version as a plain number, so a library whose schema changed
/// never reads a snapshot built against the old one.
fn schema_key(db_version: Option<i64>) -> u32 {
    db_version.and_then(|v| u32::try_from(v).ok()).unwrap_or(0)
}

fn spawn_library_load(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        match rbl_db::Library::open_installed_read_only() {
            Ok(db) => {
                let db_version = db.schema().db_version;
                let share_root = db.location().share_root.clone();
                let master_db = db.location().master_db.clone();
                let cache_path = cache_path(&app);
                // Reading 38,681 rows out of SQLCipher is 543 ms of the 680 ms
                // a start costs, and none of it gets faster — the work is the
                // decryption. A snapshot of the built columns turns the same
                // start into a sequential read.
                // Content rather than file times: rekordbox rewrites the WAL
                // without changing a row, and keying on that refused the
                // snapshot on every start it was running for.
                let content = rbl_index::content_version(&db).unwrap_or(0);
                let fingerprint = cache_path.as_ref().and_then(|_| {
                    rbl_index::cache::Fingerprint::of(&master_db, schema_key(db_version), content)
                });
                if let (Some(path), Some(fp)) = (cache_path.as_ref(), fingerprint) {
                    if let Some(library) = rbl_index::cache::load(path, fp) {
                        let load_ms =
                            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                        tracing::info!(tracks = library.len(), load_ms, "library from cache");
                        let read_only = rbl_db::is_rekordbox_running();
                        app.state::<Arc<AppState>>().set_library(
                            library, read_only, db_version, load_ms, share_root,
                        );
                        let _ = tauri::Emitter::emit(&app, "library:ready", ());
                        return;
                    }
                }
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

                        // Written after the interface is live, and only if the
                        // database has not moved since the fingerprint was
                        // taken — rekordbox may have written while we read,
                        // and a snapshot of a half-read library keyed to bytes
                        // that no longer exist would be served on a later
                        // start as though it were current.
                        if let (Some(path), Some(before)) = (cache_path.as_ref(), fingerprint) {
                            let after = rbl_index::content_version(&db).ok().and_then(|now| {
                                rbl_index::cache::Fingerprint::of(
                                    &master_db,
                                    schema_key(db_version),
                                    now,
                                )
                            });
                            if after == Some(before) {
                                let held = app.state::<Arc<AppState>>();
                                if let Ok(library) = held.library() {
                                    if let Err(e) =
                                        rbl_index::cache::save(path, &library, before)
                                    {
                                        tracing::warn!(error = %e, "could not write the library cache");
                                    }
                                }
                            }
                        }
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
/// Brings the window onto a screen that can hold it, at startup.
///
/// The configured 1800x1130 is larger than a 13-inch laptop's work area, and
/// the window-state plugin restores wherever the window was last — which may
/// be a monitor that is no longer plugged in. Either way the result is a
/// window partly or wholly out of reach, and on macOS a title bar above the
/// menu bar cannot be dragged back.
///
/// Best effort throughout: a screen that cannot be measured is a reason to
/// leave the window alone, not to fail the launch.
/// The label the window in `tauri.conf.json` gets by default.
const MAIN_WINDOW: &str = "main";

/// How long after the window appears its geometry is still corrected.
///
/// The restored position arrives asynchronously and was measured landing
/// within a second. Two gives that room without reaching as far as anything a
/// person could have done deliberately.
const SETTLE_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);


/// Puts the window back where it was, then makes sure that is on a screen.
///
/// Registered after the window-state plugin, which is told to
/// `skip_initial_state` so that the restore happens here instead — the check
/// has to follow it, and it cannot follow something this does not control.
fn window_geometry() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_window_state::{StateFlags, WindowExt};

    tauri::plugin::Builder::<tauri::Wry>::new("windowfit")
        .on_window_ready(|window| {
            if window.label() != MAIN_WINDOW {
                return;
            }
            if let Err(e) = window.restore_state(StateFlags::all()) {
                tracing::warn!(error = %e, "could not restore the window's geometry");
            }

            // Once now, which covers a first run: no saved geometry to restore,
            // and a configured size larger than the screen it opened on.
            fit_window(&window);

            // And again as the restore lands, which is the case that actually
            // bites. That move is posted to the windowing system and does not
            // arrive for the best part of a second — measured, the window
            // reported x=120 immediately, from a callback queued on the main
            // thread behind the move, and from the first `Moved` event, while
            // its real restored position was x=936 and stayed that way. No
            // single read is trustworthy, so this watches the window's own
            // events for a moment instead.
            //
            // Bounded, because every move after startup is somebody dragging
            // the window, and one that will not stay where it is put is worse
            // than one that hangs off an edge.
            let opened = std::time::Instant::now();
            let subject = window.clone();
            window.on_window_event(move |event| {
                if !matches!(event, tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)) {
                    return;
                }
                if opened.elapsed() < SETTLE_WINDOW {
                    fit_window(&subject);
                }
            });
        })
        .build()
}

fn fit_window<R: tauri::Runtime>(window: &tauri::Window<R>) {
    use tauri::{PhysicalPosition, PhysicalSize};

    // The monitor the window is on, or the primary one when it is off every
    // screen and Tauri cannot say which it belongs to.
    let monitor = match window.current_monitor() {
        Ok(Some(monitor)) => Some(monitor),
        _ => window.primary_monitor().ok().flatten(),
    };
    let Some(monitor) = monitor else { return };

    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };

    // The work area, not the whole monitor: it excludes the menu bar and the
    // Dock, which is what "on the screen" means to someone using it.
    let area = monitor.work_area();
    let available =
        windowfit::Rect::new(area.position.x, area.position.y, area.size.width, area.size.height);
    let current = windowfit::Rect::new(position.x, position.y, size.width, size.height);
    let fitted = windowfit::fit_within(current, available);
    if fitted == current {
        return;
    }

    tracing::info!(
        from = format!("{}x{} at {},{}", current.width, current.height, current.x, current.y),
        to = format!("{}x{} at {},{}", fitted.width, fitted.height, fitted.x, fitted.y),
        "window did not fit the screen"
    );
    // Size first: moving a window that is still too big only pins it to a
    // corner with the far edge still off.
    if (fitted.width, fitted.height) != (current.width, current.height) {
        let _ = window.set_size(PhysicalSize::new(fitted.width, fitted.height));
    }
    if (fitted.x, fitted.y) != (current.x, current.y) {
        let _ = window.set_position(PhysicalPosition::new(fitted.x, fitted.y));
    }
}

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
        // Puts the window back where it was: size, position, and whether it
        // was maximised. Restored before the window is shown, so it does not
        // appear at the default size and jump.
        // The saved geometry is put back by hand rather than automatically,
        // because it has to be followed by a check that it still fits a screen
        // that is present. Every way of doing that afterwards was tried
        // against a real saved state of 3600x1982 at x=936 on a 3840-wide
        // display — in `setup`, on `RunEvent::Ready`, and from a plugin hook
        // registered after this one — and all three measured the window before
        // the restore had moved it, so all three found nothing wrong.
        .plugin(tauri_plugin_window_state::Builder::default().skip_initial_state(MAIN_WINDOW).build())
        .plugin(window_geometry())
        .manage(Arc::new(AppState::new()))
        .manage(Arc::new(crate::player::Player::default()))
        .setup(|app| {
            spawn_library_load(app.handle().clone());
            app.set_menu(crate::menu::build(app.handle())?)?;
            Ok(())
        })
        .on_menu_event(|app, event| crate::menu::on_event(app, event.id().as_ref()))
        // Asynchronous, not the plain form. `wry` calls a synchronous handler
        // straight from the `WKURLSchemeHandler` callback, which is the main
        // thread on macOS, so every artwork read and every audio range would
        // block the UI thread — a fast scroll fires one per row inside the
        // frame loop. The responder lets the read happen on a blocking worker
        // instead, which is the same rule `commands.rs` already follows.
        .register_asynchronous_uri_scheme_protocol("rbl", move |ctx, request, responder| {
            // Artwork goes to the webview as an <img> rather than through
            // invoke: a JPEG blows the 64 KB IPC cap and would cost a
            // main-thread base64 decode per row.
            let state = Arc::clone(ctx.app_handle().state::<Arc<AppState>>().inner());
            tauri::async_runtime::spawn_blocking(move || {
                crate::protocol::serve(&state, &request, |response| responder.respond(response));
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_summary,
            commands::playlist_tree,
            commands::open_view,
            commands::fetch_rows,
            commands::view_ids_in_range,
            commands::track_waveform,
            commands::analyse_track,
            // Editing. Every one of these is refused while rekordbox is
            // running, re-checked immediately before the transaction.
            commands::start_link_listening,
            commands::stop_link_listening,
            commands::export_playlist,
            commands::list_devices,
            commands::track_beats,
            // The decks. The audio device is not opened until one of these
            // is called, so a window nobody has played anything in holds no
            // device at all.
            commands::deck_load,
            commands::deck_unload,
            commands::deck_play,
            commands::deck_pause,
            commands::deck_seek,
            commands::deck_scrub_begin,
            commands::deck_scrub_to,
            commands::deck_scrub_end,
            commands::app_diagnostics,
            commands::reveal_track,
            commands::set_master_level,
            commands::audio_devices,
            commands::set_audio_device,
            commands::deck_tempo,
            commands::deck_master_tempo,
            commands::set_channel_band,
            commands::set_channel_kill,
            commands::set_channel_trim,
            commands::set_crossfade,
            commands::set_eq_curve,
            commands::deck_state,
            commands::track_cues,
            commands::track_phrases,
            commands::track_vocals,
            commands::missing_tracks,
            commands::import_files,
            commands::relocate_track,
            commands::create_playlist,
            commands::create_folder,
            commands::rename_playlist,
            commands::move_playlist,
            commands::delete_playlist,
            commands::add_tracks_to_playlist,
            commands::remove_tracks_from_playlist,
            commands::reorder_playlist,
            commands::set_track_rating,
            commands::set_track_comment,
            commands::set_track_color,
        ])
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|_handle, _event| {}),
        Err(e) => {
            tracing::error!(error = %e, "fatal: could not start the application");
            std::process::exit(1);
        }
    }
}
