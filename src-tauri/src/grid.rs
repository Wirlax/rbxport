//! Beat grid editing: the GRID panel's buttons, from the playhead to the
//! user's library.
//!
//! An edit is applied to the grid in the track's `.DAT` (`rbl_anlz::grid`
//! does the arithmetic), the file is rewritten with only its `PQTZ` changed,
//! the `.EXT`'s extended grid is emptied because it described the beats
//! the `.DAT` used to have (see `Anlz::with_extended_grid_cleared`), and
//! when the tempo changed `djmdContent.BPM` follows it, which is the one
//! column a grid edit touches — the same one an analysis registers. What
//! else rekordbox writes on a grid edit of its own (`AnalysisUpdated`, most
//! likely) is **[UNKNOWN]** until a `rbl-difftool` recording of one says,
//! and is left alone rather than guessed, as `register_analysis` leaves it.
//!
//! Every edit is refused while rekordbox runs, by the same rule the writer
//! applies to the database: rekordbox rewrites analysis files of its own
//! accord, and two writers on one file is a corrupted grid.
//!
//! # Undo, and the first-edit backup
//!
//! Each track keeps an undo and a redo stack of whole grids for the
//! session, capped at [`HISTORY_CAP`]. Beyond the session there is the
//! backup: the first time this application rewrites an analysis file, the
//! file as it was is copied under the app's backup directory at its
//! share-relative path, and never overwritten after — so however many
//! sessions edit a grid, the copy is the grid rekordbox itself wrote.
//!
//! # The lock
//!
//! rekordbox's GRID panel has a padlock that stops a grid being changed.
//! Where rekordbox records it is **[UNKNOWN]**: no `djmdContent` column
//! reads as one, and a recording of locking a track would say. Until then
//! the lock is this application's own, a list of track ids beside the
//! backups, honoured by every edit here and nowhere else.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rbl_anlz::grid::{apply_from, tempo_x100, Edit};
use rbl_anlz::Beat;
use rbl_index::Library;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{blocking, reload, write_error};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::AppState;

/// Grids kept for undo per track, per session. A grid is at most a few
/// hundred kilobytes, and a hundred steps is more than a hand edits.
const HISTORY_CAP: usize = 100;

/// One edit, as the panel sends it.
///
/// The shape mirrors `rbl_anlz::grid::Edit` field for field; the panel
/// spells the kind in camelCase, as the rest of the wire does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GridEdit {
    /// Shift every beat by this many milliseconds; positive is later.
    Nudge { ms: i32 },
    Double,
    Halve,
    /// Make the beat nearest this time the downbeat.
    Downbeat { time_ms: u32 },
    /// Re-space the grid at this tempo with a beat held at the anchor.
    Tempo { bpm_x100: u16, anchor_ms: u32 },
    /// Change the tempo by hundredths of a BPM, the first beat held.
    Stretch { by_x100: i32 },
    /// Move the grid so the beat nearest this time lands on it.
    Align { time_ms: u32 },
}

impl From<GridEdit> for Edit {
    fn from(edit: GridEdit) -> Self {
        match edit {
            GridEdit::Nudge { ms } => Self::Nudge(ms),
            GridEdit::Double => Self::Double,
            GridEdit::Halve => Self::Halve,
            GridEdit::Downbeat { time_ms } => Self::Downbeat { time_ms },
            GridEdit::Tempo { bpm_x100, anchor_ms } => Self::Tempo { bpm_x100, anchor_ms },
            GridEdit::Stretch { by_x100 } => Self::Stretch { by_x100 },
            GridEdit::Align { time_ms } => Self::Align { time_ms },
        }
    }
}

/// What the panel needs to know about a track's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridStateDto {
    /// The grid's tempo x100, from its first beat; 0 without a grid.
    pub bpm_x100: u32,
    pub beats: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub locked: bool,
}

/// What one action asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridAction {
    /// An edit, to the whole grid or from the beat nearest `from_ms` on.
    Edit { edit: GridEdit, from_ms: Option<u32> },
    Undo,
    Redo,
}

/// What an action did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridOutcome {
    pub state: GridStateDto,
    /// Whether `djmdContent.BPM` changed, so the library has to be re-read.
    pub bpm_changed: bool,
    /// Whether the files were rewritten: false when the edit changed nothing.
    pub written: bool,
    /// The grid as it now stands, for the deck's metronome: milliseconds and
    /// the beat's number in its bar.
    pub beats: Vec<(u32, u8)>,
}

#[derive(Debug, Default)]
struct History {
    undo: Vec<Vec<Beat>>,
    redo: Vec<Vec<Beat>>,
}

/// The session's grid editing state: histories, the lock list, and which
/// files have been backed up. Managed by the app beside `AppState`.
#[derive(Debug)]
pub struct GridEditor {
    histories: parking_lot::Mutex<HashMap<String, History>>,
    /// Loaded from `locks_path` on first use; `None` until then.
    locks: parking_lot::Mutex<Option<BTreeSet<String>>>,
    locks_path: PathBuf,
    /// Where analysis files are copied before their first rewrite.
    backup_dir: PathBuf,
    /// Files checked for a backup this session, so the check is one per file.
    backed_up: parking_lot::Mutex<HashSet<PathBuf>>,
}

impl Default for GridEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl GridEditor {
    /// The editor for the installed library: locks and backups under the
    /// app's own data directory, beside the database backups.
    pub fn new() -> Self {
        let root = dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("rbxport");
        Self::at(&root)
    }

    /// An editor keeping its lock list and backups under `root`.
    pub fn at(root: &Path) -> Self {
        Self {
            histories: parking_lot::Mutex::new(HashMap::new()),
            locks: parking_lot::Mutex::new(None),
            locks_path: root.join("grid-locks.json"),
            backup_dir: root.join("backups/analysis"),
            backed_up: parking_lot::Mutex::new(HashSet::new()),
        }
    }

    /// Whether the track's grid is locked against editing.
    pub fn is_locked(&self, track: &str) -> bool {
        self.with_locks(|locks| locks.contains(track))
    }

    /// Locks or unlocks a track's grid, and records it for the next session.
    pub fn set_locked(&self, track: &str, on: bool) -> AppResult<()> {
        let mut held = self.locks.lock();
        let locks = held.get_or_insert_with(|| read_locks(&self.locks_path));
        let changed = if on { locks.insert(track.to_owned()) } else { locks.remove(track) };
        if !changed {
            return Ok(());
        }
        let text = serde_json::to_string(&locks.iter().collect::<Vec<_>>())
            .map_err(|e| AppError::internal(format!("the lock list could not be encoded: {e}")))?;
        if let Some(parent) = self.locks_path.parent() {
            // perf-ok: called from the commands' blocking closures only
            std::fs::create_dir_all(parent).map_err(|e| lock_error(&self.locks_path, &e))?;
        }
        write_atomically(&self.locks_path, text.as_bytes()).map_err(|e| lock_error(&self.locks_path, &e))
    }

    fn with_locks<T>(&self, f: impl FnOnce(&BTreeSet<String>) -> T) -> T {
        let mut held = self.locks.lock();
        f(held.get_or_insert_with(|| read_locks(&self.locks_path)))
    }

    fn state_for(&self, track: &str, beats: &[Beat]) -> GridStateDto {
        let histories = self.histories.lock();
        let history = histories.get(track);
        GridStateDto {
            bpm_x100: u32::from(tempo_x100(beats)),
            beats: u32::try_from(beats.len()).unwrap_or(u32::MAX),
            can_undo: history.is_some_and(|h| !h.undo.is_empty()),
            can_redo: history.is_some_and(|h| !h.redo.is_empty()),
            locked: self.is_locked(track),
        }
    }

    /// Copies a file under the backup directory at `relative`, once: a
    /// copy already there is the file as rekordbox wrote it, and stays.
    fn back_up(&self, file: &Path, relative: &str, extension: &str) -> AppResult<()> {
        if self.backed_up.lock().contains(file) || !file.is_file() {
            return Ok(());
        }
        let copy = self
            .backup_dir
            .join(relative.trim_start_matches(['/', '\\']))
            .with_extension(extension);
        if !copy.exists() {
            if let Some(parent) = copy.parent() {
                // perf-ok: called from the commands' blocking closures only
                std::fs::create_dir_all(parent).map_err(|e| file_error("backed up", &copy, &e))?;
            }
            std::fs::copy(file, &copy).map_err(|e| file_error("backed up", &copy, &e))?;
        }
        self.backed_up.lock().insert(file.to_path_buf());
        Ok(())
    }
}

fn read_locks(path: &Path) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(path) else { return BTreeSet::new() };
    match serde_json::from_str::<Vec<String>>(&text) {
        Ok(ids) => ids.into_iter().collect(),
        Err(e) => {
            // A list that cannot be read locks nothing, and says so once
            // rather than every edit; the next lock rewrites it whole.
            tracing::warn!(path = %path.display(), error = %e, "the grid lock list could not be read");
            BTreeSet::new()
        }
    }
}

fn lock_error(path: &Path, error: &std::io::Error) -> AppError {
    AppError::new(ErrorKind::Internal, "The grid lock could not be saved.")
        .with_detail(format!("{}: {error}", path.display()))
}

fn file_error(what: &str, path: &Path, error: &std::io::Error) -> AppError {
    AppError::new(ErrorKind::Internal, format!("The analysis file could not be {what}."))
        .with_detail(format!("{}: {error}", path.display()))
}

/// Writes a file whole, through a sibling and a rename, so a crash midway
/// leaves the old file rather than half of the new one.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::durable::write(path, bytes)
}

/// A track's analysis files, resolved.
struct Files {
    relative: String,
    dat: PathBuf,
    ext: PathBuf,
}

fn files_of(library: &Library, share: &Path, track: &str) -> AppResult<Files> {
    let Some(row) = library.row_of(track) else {
        return Err(AppError::new(ErrorKind::NotFound, "That track is not in the library."));
    };
    let relative = library.analysis_path.get(row as usize);
    if relative.is_empty() {
        return Err(AppError::new(ErrorKind::NotFound, "That track has not been analysed, so it has no grid to edit."));
    }
    let dat = rbl_anlz::resolve(share, relative);
    let ext = rbl_anlz::sibling(&dat, "EXT");
    Ok(Files { relative: relative.to_owned(), dat, ext })
}

fn read_dat(dat: &Path) -> AppResult<(rbl_anlz::Anlz, Vec<Beat>)> {
    let parsed = rbl_anlz::Anlz::read(dat).map_err(|e| {
        AppError::new(ErrorKind::NotFound, "That track's analysis file could not be read.")
            .with_detail(format!("{}: {e}", dat.display()))
    })?;
    let Some(beats) = parsed.beat_grid().filter(|beats| !beats.is_empty()) else {
        return Err(AppError::new(ErrorKind::NotFound, "That track has no beat grid to edit."));
    };
    Ok((parsed, beats))
}

/// The grid as the panel sees it, without changing anything.
pub fn state_of(editor: &GridEditor, library: &Library, share: &Path, track: &str) -> AppResult<GridStateDto> {
    let files = files_of(library, share, track)?;
    let (_, beats) = read_dat(&files.dat)?;
    Ok(editor.state_for(track, &beats))
}

/// Applies one action to a track's grid: the files, the history and, through
/// `set_bpm`, the database row when the tempo changed.
///
/// Apart from the commands so it can be tested against a fixture in a
/// tempdir without a Tauri app. `set_bpm` is called with the new tempo x100
/// only when it differs from the old; it opens the writer, which is what
/// takes the database backup and re-checks the gate. If it fails, the files
/// are put back as they were, so the row and the grid never disagree.
pub fn apply(
    editor: &GridEditor,
    library: &Library,
    location: &rbl_db::LibraryLocation,
    track: &str,
    action: GridAction,
    set_bpm: &mut dyn FnMut(u32) -> AppResult<()>,
) -> AppResult<GridOutcome> {
    // The writer's own rule, applied to the files as well as the row: a
    // fixture in a tempdir is not the installed library, so its tests run
    // whether or not rekordbox is open.
    if let Some(reason) = rbl_db::write_refusal_reason(
        location.is_real_install,
        std::env::var_os("RB_LITE_TEST").is_some(),
        rbl_db::is_rekordbox_running(),
    ) {
        return Err(AppError::new(ErrorKind::ReadOnly, reason));
    }
    let files = files_of(library, &location.share_root, track)?;
    let (parsed, beats) = read_dat(&files.dat)?;

    let next: Vec<Beat> = match action {
        GridAction::Edit { edit, from_ms } => {
            if editor.is_locked(track) {
                return Err(AppError::new(ErrorKind::ReadOnly, "The beat grid is locked. Unlock it to edit."));
            }
            let next = apply_from(&beats, from_ms, edit.into());
            if next.is_empty() {
                return Err(AppError::new(ErrorKind::Malformed, "That edit would leave the track without a beat."));
            }
            if next == beats {
                return Ok(GridOutcome {
                    state: editor.state_for(track, &beats),
                    bpm_changed: false,
                    written: false,
                    beats: wire_beats(&beats),
                });
            }
            next
        }
        GridAction::Undo | GridAction::Redo => {
            let mut histories = editor.histories.lock();
            let history = histories.entry(track.to_owned()).or_default();
            let (from, to) = if action == GridAction::Undo {
                (&mut history.undo, &mut history.redo)
            } else {
                (&mut history.redo, &mut history.undo)
            };
            let Some(previous) = from.pop() else {
                let what = if action == GridAction::Undo { "undo" } else { "redo" };
                return Err(AppError::new(ErrorKind::NotFound, format!("Nothing to {what}.")));
            };
            to.push(beats.clone());
            previous
        }
    };

    // The files, the original kept aside until the row agrees.
    let original = std::fs::read(&files.dat).map_err(|e| file_error("read", &files.dat, &e))?;
    editor.back_up(&files.dat, &files.relative, "DAT")?;
    editor.back_up(&files.ext, &files.relative, "EXT")?;
    write_atomically(&files.dat, &parsed.with_beat_grid(&next)).map_err(|e| file_error("written", &files.dat, &e))?;
    if let Ok(ext) = rbl_anlz::Anlz::read(&files.ext) {
        if let Some(cleared) = ext.with_extended_grid_cleared() {
            write_atomically(&files.ext, &cleared).map_err(|e| file_error("written", &files.ext, &e))?;
        }
    }

    let old_bpm = u32::from(tempo_x100(&beats));
    let new_bpm = u32::from(tempo_x100(&next));
    let bpm_changed = new_bpm != old_bpm;
    if bpm_changed {
        if let Err(refused) = set_bpm(new_bpm) {
            // The grid goes back with the row it still matches. A restore
            // that fails leaves the original error in place: it is the one
            // that explains what happened, and the backup holds the file.
            if let Err(e) = write_atomically(&files.dat, &original) {
                tracing::error!(path = %files.dat.display(), error = %e, "the grid could not be put back");
            }
            return Err(refused);
        }
    }

    if let GridAction::Edit { .. } = action {
        let mut histories = editor.histories.lock();
        let history = histories.entry(track.to_owned()).or_default();
        history.redo.clear();
        history.undo.push(beats);
        if history.undo.len() > HISTORY_CAP {
            history.undo.remove(0);
        }
    }

    Ok(GridOutcome {
        state: editor.state_for(track, &next),
        bpm_changed,
        written: true,
        beats: wire_beats(&next),
    })
}

fn wire_beats(beats: &[Beat]) -> Vec<(u32, u8)> {
    beats.iter().map(|beat| (beat.time_ms, u8::try_from(beat.beat_number).unwrap_or(0))).collect()
}

/// Runs one action under `blocking` (a `spawn_blocking` thread: the files
/// and the row are disk work), then tells the interface and the deck what
/// changed.
#[allow(clippy::too_many_arguments, reason = "three managed states, the app, and what the command was given")]
async fn run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    player: State<'_, Arc<crate::player::Player>>,
    editor: State<'_, Arc<GridEditor>>,
    name: &'static str,
    track: String,
    action: GridAction,
    deck: Option<String>,
) -> AppResult<GridStateDto> {
    let library = state.library()?;
    let location = state.location()?;
    let state = Arc::clone(&state);
    let editor = Arc::clone(&editor);
    let outcome = {
        let state = Arc::clone(&state);
        let track = track.clone();
        blocking(name, move || {
            let mut set_bpm = |bpm_x100: u32| {
                let track = track.clone();
                state.write(|writer| writer.set_bpm_x100(&track, bpm_x100).map(|_| ())).map_err(write_error)
            };
            apply(&editor, &library, &location, &track, action, &mut set_bpm)
        })
        .await?
    };
    if !outcome.written {
        return Ok(outcome.state);
    }
    // The deck's metronome plays the grid it was given on load; give it this one.
    if let (Some(deck), Some(engine)) = (deck, player.opened()) {
        let grid: Vec<(u32, bool)> = outcome.beats.iter().map(|&(ms, number)| (ms, number == 1)).collect();
        engine.set_metronome_grid(crate::player::deck_of(&deck), &grid);
    }
    // The track's id, inside the event cap: every deck showing the track
    // refetches its grid. A changed tempo is a changed column, so the
    // library is re-read as it is after any other track edit.
    let _ = tauri::Emitter::emit(&app, "grid:changed", &track);
    if outcome.bpm_changed {
        reload(app, state).await?;
    }
    Ok(outcome.state)
}

/// The grid as the panel shows it: tempo, beat count, undo, redo, lock.
#[tauri::command]
pub async fn grid_state(
    state: State<'_, Arc<AppState>>,
    editor: State<'_, Arc<GridEditor>>,
    track: String,
) -> AppResult<GridStateDto> {
    let library = state.library()?;
    let share = state.share_root();
    let editor = Arc::clone(&editor);
    blocking("grid_state", move || state_of(&editor, &library, &share, &track)).await
}

/// One grid edit. `from_ms` names the beat from which it applies — the CUT
/// point, or the playhead for the from-here buttons — and `deck` the deck
/// the track is loaded on, so its metronome follows.
#[tauri::command]
#[allow(clippy::too_many_arguments, reason = "a command takes its states and its arguments flat")]
pub async fn grid_edit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    player: State<'_, Arc<crate::player::Player>>,
    editor: State<'_, Arc<GridEditor>>,
    track: String,
    edit: GridEdit,
    from_ms: Option<u32>,
    deck: Option<String>,
) -> AppResult<GridStateDto> {
    run(app, state, player, editor, "grid_edit", track, GridAction::Edit { edit, from_ms }, deck).await
}

#[tauri::command]
pub async fn grid_undo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    player: State<'_, Arc<crate::player::Player>>,
    editor: State<'_, Arc<GridEditor>>,
    track: String,
    deck: Option<String>,
) -> AppResult<GridStateDto> {
    run(app, state, player, editor, "grid_undo", track, GridAction::Undo, deck).await
}

#[tauri::command]
pub async fn grid_redo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    player: State<'_, Arc<crate::player::Player>>,
    editor: State<'_, Arc<GridEditor>>,
    track: String,
    deck: Option<String>,
) -> AppResult<GridStateDto> {
    run(app, state, player, editor, "grid_redo", track, GridAction::Redo, deck).await
}

/// Locks or unlocks a track's grid. Nothing in the library changes: see the
/// module docs for where the lock lives.
#[tauri::command]
pub async fn grid_lock(
    state: State<'_, Arc<AppState>>,
    editor: State<'_, Arc<GridEditor>>,
    track: String,
    on: bool,
) -> AppResult<GridStateDto> {
    let library = state.library()?;
    let share = state.share_root();
    let editor = Arc::clone(&editor);
    blocking("grid_lock", move || {
        editor.set_locked(&track, on)?;
        state_of(&editor, &library, &share, &track)
    })
    .await
}

#[cfg(test)]
mod tests {
    // Every test builds its own library and share tree in a tempdir, the way
    // the cue tests do; `RB_LITE_TEST` cannot reach a fixture.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use rbl_anlz::AnlzBuilder;
    use rbl_db::fixture::{self, track_id, Shape};
    use rbl_db::{Library as Db, OpenMode};

    const RELATIVE: &str = "/PIONEER/USBANLZ/ab1/cd2/ANLZ0000.DAT";

    struct Fixture {
        dir: tempfile::TempDir,
        location: rbl_db::LibraryLocation,
        library: Library,
        editor: GridEditor,
        /// Every BPM the action asked the row to take, in order.
        bpms: Vec<u32>,
    }

    /// Eight beats at 120 BPM from 500 ms, numbered from the first.
    fn grid() -> Vec<Beat> {
        (0..8)
            .map(|i| Beat { beat_number: (i % 4) + 1, tempo_x100: 12_000, time_ms: 500 + u32::from(i) * 500 })
            .collect()
    }

    fn open() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let location = fixture::build(dir.path(), Shape::default()).expect("build the fixture");
        fixture::set_analysis_path(&location, 1, RELATIVE).unwrap();
        let dat = rbl_anlz::resolve(&location.share_root, RELATIVE);
        std::fs::create_dir_all(dat.parent().unwrap()).unwrap();
        let mut file = AnlzBuilder::new();
        file.path("/x.mp3").beat_grid(&grid()).vbr_table_zero().cue_lists(false);
        std::fs::write(&dat, file.finish()).unwrap();
        // An `.EXT` with a filled extended grid, as rekordbox writes one.
        let mut ext = AnlzBuilder::new();
        ext.path("/x.mp3").cue_lists(true);
        let mut header = vec![0_u8; 44];
        header[4..8].copy_from_slice(&0x0100_0002_u32.to_be_bytes());
        ext.raw(rbl_core::FourCc::new(b"PQT2"), header, vec![0, 1, 0, 2, 0, 3]);
        std::fs::write(dat.with_extension("EXT"), ext.finish()).unwrap();

        let db = Db::open(location.clone(), OpenMode::ReadOnly).expect("read-only");
        let (library, _) = rbl_index::load(&db).expect("index");
        let editor = GridEditor::at(&dir.path().join("app"));
        Fixture { dir, location, library, editor, bpms: Vec::new() }
    }

    impl Fixture {
        fn track() -> String {
            track_id(1)
        }

        fn run(&mut self, action: GridAction) -> AppResult<GridOutcome> {
            let track = Self::track();
            let bpms = &mut self.bpms;
            let mut set_bpm = |bpm: u32| {
                bpms.push(bpm);
                Ok(())
            };
            apply(&self.editor, &self.library, &self.location, &track, action, &mut set_bpm)
        }

        fn edit(&mut self, edit: GridEdit) -> GridOutcome {
            self.run(GridAction::Edit { edit, from_ms: None }).unwrap()
        }

        fn dat(&self) -> rbl_anlz::Anlz {
            rbl_anlz::Anlz::read(&rbl_anlz::resolve(&self.location.share_root, RELATIVE)).unwrap()
        }

        fn ext(&self) -> rbl_anlz::Anlz {
            rbl_anlz::Anlz::read(&rbl_anlz::resolve(&self.location.share_root, RELATIVE).with_extension("EXT")).unwrap()
        }

        fn times(&self) -> Vec<u32> {
            self.dat().beat_grid().unwrap().iter().map(|b| b.time_ms).collect()
        }
    }

    #[test]
    fn the_edit_wire_shape_is_a_tagged_kind() {
        let edit: GridEdit = serde_json::from_str(r#"{"kind":"nudge","ms":-3}"#).unwrap();
        assert_eq!(edit, GridEdit::Nudge { ms: -3 });
        let edit: GridEdit = serde_json::from_str(r#"{"kind":"tempo","bpmX100":12800,"anchorMs":500}"#).unwrap();
        assert_eq!(edit, GridEdit::Tempo { bpm_x100: 12_800, anchor_ms: 500 });
        assert!(serde_json::from_str::<GridEdit>(r#"{"kind":"reset"}"#).is_err());
        assert_eq!(Edit::from(GridEdit::Halve), Edit::Halve);
    }

    #[test]
    fn a_nudge_rewrites_the_grid_empties_the_extended_grid_and_leaves_the_row() {
        let mut f = open();
        let before_ext = f.ext();
        assert!(!before_ext.section(b"PQT2").unwrap().payload.is_empty());

        let outcome = f.edit(GridEdit::Nudge { ms: 20 });
        assert!(outcome.written);
        assert!(!outcome.bpm_changed);
        assert!(f.bpms.is_empty(), "the tempo did not change, so the row is untouched");
        assert_eq!(f.times(), vec![520, 1020, 1520, 2020, 2520, 3020, 3520, 4020]);
        assert_eq!(outcome.beats[0], (520, 1));
        assert_eq!(outcome.state, GridStateDto { bpm_x100: 12_000, beats: 8, can_undo: true, can_redo: false, locked: false });

        // Only the grid changed in the `.DAT`, and only `PQT2` in the `.EXT`.
        let dat = f.dat();
        assert_eq!(dat.path().as_deref(), Some("/x.mp3"));
        assert_eq!(dat.sections.len(), 5);
        let ext = f.ext();
        assert!(ext.section(b"PQT2").unwrap().payload.is_empty());
        assert_eq!(ext.sections.len(), before_ext.sections.len());
        assert_eq!(ext.sections[0], before_ext.sections[0]);
    }

    #[test]
    fn a_tempo_change_sets_the_row_and_undo_sets_it_back() {
        let mut f = open();
        let outcome = f.edit(GridEdit::Stretch { by_x100: 50 });
        assert!(outcome.bpm_changed);
        assert_eq!(f.bpms, vec![12_050]);
        assert_eq!(outcome.state.bpm_x100, 12_050);

        let undone = f.run(GridAction::Undo).unwrap();
        assert_eq!(f.bpms, vec![12_050, 12_000]);
        assert_eq!(f.times(), (0..8).map(|i| 500 + i * 500).collect::<Vec<_>>());
        assert_eq!(undone.state, GridStateDto { bpm_x100: 12_000, beats: 8, can_undo: false, can_redo: true, locked: false });

        let redone = f.run(GridAction::Redo).unwrap();
        assert_eq!(f.bpms, vec![12_050, 12_000, 12_050]);
        assert!(redone.state.can_undo);
        assert!(!redone.state.can_redo);

        // A new edit forgets what could have been redone.
        f.run(GridAction::Undo).unwrap();
        assert!(f.editor.state_for(&Fixture::track(), &grid()).can_redo);
        let edited = f.edit(GridEdit::Nudge { ms: 1 });
        assert!(!edited.state.can_redo, "a fresh edit ends the redo stack");
        let err = f.run(GridAction::Redo).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }

    #[test]
    fn nothing_to_undo_is_said_not_written() {
        let mut f = open();
        let err = f.run(GridAction::Undo).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert_eq!(f.times()[0], 500);
    }

    #[test]
    fn an_edit_that_changes_nothing_writes_nothing() {
        let mut f = open();
        let stamp = std::fs::metadata(rbl_anlz::resolve(&f.location.share_root, RELATIVE)).unwrap().modified().unwrap();
        let outcome = f.edit(GridEdit::Nudge { ms: 0 });
        assert!(!outcome.written);
        assert!(!outcome.state.can_undo);
        let after = std::fs::metadata(rbl_anlz::resolve(&f.location.share_root, RELATIVE)).unwrap().modified().unwrap();
        assert_eq!(stamp, after);
    }

    #[test]
    fn an_edit_from_a_point_keeps_the_head() {
        let mut f = open();
        f.run(GridAction::Edit { edit: GridEdit::Nudge { ms: 100 }, from_ms: Some(2500) }).unwrap();
        assert_eq!(f.times(), vec![500, 1000, 1500, 2000, 2600, 3100, 3600, 4100]);
    }

    #[test]
    fn a_row_that_refuses_the_tempo_gets_its_grid_back() {
        let mut f = open();
        let mut refuse = |_bpm: u32| Err(AppError::new(ErrorKind::ReadOnly, "no"));
        let err = apply(
            &f.editor,
            &f.library,
            &f.location,
            &Fixture::track(),
            GridAction::Edit { edit: GridEdit::Double, from_ms: None },
            &mut refuse,
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::ReadOnly);
        assert_eq!(f.times().len(), 8, "the doubled grid was put back");
        assert!(!f.run(GridAction::Undo).is_ok_and(|o| o.written), "nothing to undo either");
    }

    #[test]
    fn the_lock_refuses_edits_and_outlives_the_editor() {
        let mut f = open();
        f.editor.set_locked(&Fixture::track(), true).unwrap();
        let err = f.edit_result(GridEdit::Halve).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ReadOnly);
        assert_eq!(f.times().len(), 8);
        assert!(state_of(&f.editor, &f.library, &f.location.share_root, &Fixture::track()).unwrap().locked);

        let again = GridEditor::at(&f.dir.path().join("app"));
        assert!(again.is_locked(&Fixture::track()));
        again.set_locked(&Fixture::track(), false).unwrap();
        assert!(!GridEditor::at(&f.dir.path().join("app")).is_locked(&Fixture::track()));
        assert!(!again.is_locked("other"));
    }

    impl Fixture {
        fn edit_result(&mut self, edit: GridEdit) -> AppResult<GridOutcome> {
            self.run(GridAction::Edit { edit, from_ms: None })
        }
    }

    #[test]
    fn the_first_rewrite_backs_the_files_up_and_later_ones_do_not_touch_the_copy() {
        let mut f = open();
        f.edit(GridEdit::Nudge { ms: 5 });
        let copy = f.dir.path().join("app/backups/analysis/PIONEER/USBANLZ/ab1/cd2/ANLZ0000.DAT");
        let kept = rbl_anlz::Anlz::read(&copy).unwrap().beat_grid().unwrap();
        assert_eq!(kept[0].time_ms, 500, "the copy is the grid before any edit");
        assert!(copy.with_extension("EXT").is_file());
        f.edit(GridEdit::Nudge { ms: 5 });
        // A second session finds the copy already there and leaves it.
        let later = GridEditor::at(&f.dir.path().join("app"));
        let mut none = |_bpm: u32| Ok(());
        apply(&later, &f.library, &f.location, &Fixture::track(), GridAction::Edit { edit: GridEdit::Nudge { ms: 5 }, from_ms: None }, &mut none).unwrap();
        assert_eq!(rbl_anlz::Anlz::read(&copy).unwrap().beat_grid().unwrap()[0].time_ms, 500);
        assert_eq!(f.times()[0], 515);
    }

    #[test]
    fn an_edit_past_the_history_cap_drops_the_oldest_grid_so_the_first_one_cannot_be_undone_to() {
        let mut f = open();
        // One edit more than the cap fits: each pushes the grid it replaced,
        // and the push past the cap takes the front of the stack off.
        let edits = HISTORY_CAP + 1;
        for _ in 0..edits {
            f.edit(GridEdit::Nudge { ms: 1 });
        }
        let cap = u32::try_from(HISTORY_CAP).unwrap();
        assert_eq!(f.times()[0], 500 + cap + 1);
        assert_eq!(f.editor.histories.lock()[&Fixture::track()].undo.len(), HISTORY_CAP, "the stack stops at the cap");

        // Undoing as far as the stack goes lands on the grid after the first
        // edit, not on the grid the track started with: the entry holding it
        // was the one dropped, and nothing else keeps it. The backup on disk
        // is what the original grid survives in.
        for _ in 0..HISTORY_CAP {
            f.run(GridAction::Undo).unwrap();
        }
        assert_eq!(f.times()[0], 501, "one edit in, not back at 500");
        let err = f.run(GridAction::Undo).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(!f.editor.state_for(&Fixture::track(), &grid()).can_undo);

        // Everything undone is still redoable: the cap is on the undo stack
        // after a push, and a redo puts grids back without being capped.
        assert_eq!(f.editor.histories.lock()[&Fixture::track()].redo.len(), HISTORY_CAP);
        for _ in 0..HISTORY_CAP {
            f.run(GridAction::Redo).unwrap();
        }
        assert_eq!(f.times()[0], 500 + cap + 1);
        assert_eq!(f.editor.histories.lock()[&Fixture::track()].undo.len(), HISTORY_CAP);
        assert!(f.bpms.is_empty(), "a nudge never changes the tempo, however many there are");
    }

    #[test]
    fn a_track_without_analysis_has_no_grid_to_edit() {
        let mut f = open();
        let mut none = |_bpm: u32| Ok(());
        let err = apply(&f.editor, &f.library, &f.location, &track_id(2), GridAction::Edit { edit: GridEdit::Double, from_ms: None }, &mut none).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        let err = state_of(&f.editor, &f.library, &f.location.share_root, "no-such-track").unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(f.run(GridAction::Redo).is_err());
    }
}
