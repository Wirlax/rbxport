//! The native application menu.
//!
//! Built in Rust so it is a real macOS menu bar rather than a strip drawn in
//! the window — which is what rekordbox has, and what a Mac user expects to
//! find their keyboard shortcuts in.
//!
//! Every label comes from `src/i18n/en.json`, which was transcribed from the
//! left-hand keys of rekordbox's `german.lang` (`english.lang` is a stub).
//! Compiled in rather than duplicated here, so the wording cannot drift from
//! the rest of the interface.
//!
//! Only items that do something are here. rekordbox's File menu also offers
//! XML collection export, its Help menu links to Pioneer's manuals, and its
//! View menu toggles panels we have not built; an item that greys out forever
//! or opens somebody else's website is worse than an absent one.

use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The transcribed English strings, compiled in at build time.
const STRINGS: &str = include_str!("../../src/i18n/en.json");

/// Looks a label up, falling back to the key — which is itself the English
/// string, so a missing entry reads correctly rather than blank.
fn label(key: &str) -> String {
    static PARSED: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let strings = PARSED.get_or_init(|| serde_json::from_str(STRINGS).unwrap_or(serde_json::Value::Null));
    strings
        .get("menu")
        .and_then(|group| group.get(key))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(key)
        .to_owned()
}

/// The event a menu item sends to the frontend.
///
/// One event with the item's id rather than an event per item: the frontend
/// maps ids to actions in one place, and adding an item does not mean adding
/// another listener.
pub const EVENT: &str = "menu";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let settings = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let import = MenuItemBuilder::with_id("import", label("Import"))
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let missing = MenuItemBuilder::with_id("missing", label("Missing File Manager")).build(app)?;

    // The application menu, whose first item macOS names after the app.
    let application = SubmenuBuilder::new(app, "rekordbox-lite")
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .separator()
        .item(&settings)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    let file = SubmenuBuilder::new(app, label("File"))
        .item(&import)
        .item(&missing)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    let view = SubmenuBuilder::new(app, label("View"))
        .item(
            &MenuItemBuilder::with_id("info", label("Information Window"))
                .accelerator("CmdOrCtrl+I")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("sub", label("Sub-Browser Window"))
                .accelerator("CmdOrCtrl+B")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("fullscreen", label("Full screen"))
                // rekordbox's own, from its Export key map.
                .accelerator("Shift+CmdOrCtrl+F")
                .build(app)?,
        )
        .separator()
        // The layout switch, on the keys rekordbox's Export preset gives it.
        .item(
            &MenuItemBuilder::with_id("layout-one", label("1 Player"))
                .accelerator("CmdOrCtrl+7")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("layout-two", label("2 Players"))
                .accelerator("CmdOrCtrl+8")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("layout-simple", label("Simple Player"))
                .accelerator("CmdOrCtrl+9")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("layout-browser", label("Full Browser"))
                .accelerator("CmdOrCtrl+0")
                .build(app)?,
        )
        .build()?;

    // Edit is predefined: without it the standard clipboard shortcuts do not
    // reach the webview's text fields on macOS, and renaming a playlist stops
    // accepting paste.
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    Menu::with_items(app, &[&application, &file, &edit, &view])
}

/// Handles a menu click.
///
/// Anything the shell can do itself is done here; everything else goes to the
/// frontend as one event carrying the item's id.
pub fn on_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if id == "fullscreen" {
        if let Some(window) = app.get_webview_window("main") {
            let full = window.is_fullscreen().unwrap_or(false);
            let _ = window.set_fullscreen(!full);
        }
        return;
    }
    let _ = app.emit(EVENT, id);
}
