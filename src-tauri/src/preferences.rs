//! The Preferences window.
//!
//! rekordbox opens Preferences as a window of its own, with a title bar to
//! drag it by, beside the main one; so does this. It loads the same bundle
//! at `index.html#preferences/<pane>`, which the frontend renders as the
//! Preferences alone. The two windows share `localStorage`, so a choice made
//! here reaches the main window through its `storage` event.

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{AppError, AppResult, ErrorKind};

pub const WINDOW: &str = "preferences";

/// The window's content, from the capture: 798 by 828pt with its 28pt
/// title bar, which macOS draws itself — see design/tokens `prefW`, `prefH`.
const WIDTH: f64 = 798.0;
const HEIGHT: f64 = 800.0;

/// Opens the window on `pane`, or brings the open one to the front and turns
/// it to that pane.
#[tauri::command]
// Tauri hands the handle and the argument over by value; the command owns both.
#[allow(clippy::needless_pass_by_value)]
pub fn open_preferences(app: tauri::AppHandle, pane: String) -> AppResult<()> {
    // Only a pane name reaches the URL: anything else is not one.
    let pane: String = pane.chars().filter(char::is_ascii_alphanumeric).collect();
    if let Some(window) = app.get_webview_window(WINDOW) {
        // The window reads its pane from the hash, and listens for it to change.
        let _ = window.eval(format!("location.hash = '#preferences/{pane}'"));
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        WINDOW,
        WebviewUrl::App(format!("index.html#preferences/{pane}").into()),
    )
    .title("Preferences")
    .inner_size(WIDTH, HEIGHT)
    .min_inner_size(WIDTH, 480.0)
    .resizable(true)
    .accept_first_mouse(true)
    .build()
    .map(|_| ())
    .map_err(|e| AppError::new(ErrorKind::Internal, format!("the Preferences window could not open: {e}")))
}
