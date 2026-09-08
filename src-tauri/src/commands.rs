//! Tauri commands.
//!
//! Each is a thin adapter. Anything that touches the index runs on a blocking
//! thread via [`blocking`], which serves two rules at once: the async runtime
//! is never blocked, and a panic inside a command surfaces as an `AppError`
//! instead of taking the process down.

use std::sync::Arc;

use rbl_index::Library;
use tauri::State;

use crate::link::LinkStatusDto;
use crate::dto::{
    CueDto, ImportReportDto, LibrarySummaryDto, MissingTrackDto, MissingTracksDto, RowDto,
    TreeNodeDto, ViewHandleDto, ViewSpecDto,
};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::{rows_to_dto, spec_from_wire, AppState};

/// Rows per request. The frontend asks a page at a time; this bound is what
/// keeps a response inside the 64 KB cap.
const MAX_ROWS: u32 = 128;

/// Runs `f` on a blocking thread and converts a panic there into an `AppError`.
async fn blocking<T, F>(name: &'static str, f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    // `run_command` catches an unwind inside the worker so the message names the
    // command; the JoinError arm is the backstop if the thread dies some other way.
    match tauri::async_runtime::spawn_blocking(move || {
        crate::error::run_command(name, std::panic::AssertUnwindSafe(f))
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(command = name, error = %e, "command panicked");
            Err(AppError::internal(format!("{name} panicked: {e}")))
        }
    }
}

#[tauri::command]
pub async fn library_summary(state: State<'_, Arc<AppState>>) -> AppResult<LibrarySummaryDto> {
    let library = state.library()?;
    let (read_only, db_version, load_ms, _generation) = state.summary();
    blocking("library_summary", move || {
        Ok(LibrarySummaryDto {
            track_count: u32::try_from(library.len()).unwrap_or(u32::MAX),
            playlist_count: u32::try_from(library.playlists().len()).unwrap_or(u32::MAX),
            read_only,
            db_version,
            load_ms,
        })
    })
    .await
}

#[tauri::command]
pub async fn playlist_tree(state: State<'_, Arc<AppState>>) -> AppResult<Vec<TreeNodeDto>> {
    let library = state.library()?;
    blocking("playlist_tree", move || Ok(build_tree(&library))).await
}

fn build_tree(library: &Library) -> Vec<TreeNodeDto> {
    let playlists = library.playlists();
    let mut nodes = vec![
        TreeNodeDto {
            id: "all".into(),
            name: "All Tracks".into(),
            kind: "allTracks",
            depth: 0,
            expanded: None,
            child_count: Some(u32::try_from(library.len()).unwrap_or(u32::MAX)),
        },
        TreeNodeDto {
            id: "playlists".into(),
            name: "Playlists".into(),
            kind: "collection",
            depth: 0,
            expanded: Some(true),
            child_count: Some(u32::try_from(playlists.len()).unwrap_or(u32::MAX)),
        },
    ];

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); playlists.len()];
    let mut roots: Vec<usize> = Vec::new();
    for index in 0..playlists.len() {
        match playlists.parent.get(index).copied() {
            Some(parent) if parent != rbl_index::NO_ID && (parent as usize) < playlists.len() => {
                if let Some(bucket) = children.get_mut(parent as usize) {
                    bucket.push(index);
                }
            }
            _ => roots.push(index),
        }
    }

    // Iterative, with a visited set: a corrupt parent cycle must not recurse
    // forever or blow the stack.
    let mut stack: Vec<(usize, u32)> = roots.iter().rev().map(|&i| (i, 1_u32)).collect();
    let mut visited = vec![false; playlists.len()];
    while let Some((index, depth)) = stack.pop() {
        if visited.get(index).copied().unwrap_or(true) {
            continue;
        }
        if let Some(slot) = visited.get_mut(index) {
            *slot = true;
        }
        let kids = children.get(index).map_or(0, Vec::len);
        let members = playlists.members.get(index).map_or(0, Vec::len);
        nodes.push(TreeNodeDto {
            id: playlists.ids.get(index).copied().unwrap_or(0).to_string(),
            name: playlists.name(index).to_owned(),
            kind: if kids > 0 { "folder" } else { "playlist" },
            depth,
            expanded: if kids > 0 { Some(depth < 2) } else { None },
            child_count: Some(
                u32::try_from(if kids > 0 { kids } else { members }).unwrap_or(u32::MAX),
            ),
        });
        if let Some(kids) = children.get(index) {
            for &child in kids.iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
    nodes
}

#[tauri::command]
pub async fn open_view(state: State<'_, Arc<AppState>>, spec: ViewSpecDto) -> AppResult<ViewHandleDto> {
    let library = state.library()?;
    let parsed = spec_from_wire(&library, &spec);
    // Sorting and filtering happen here, so this is the one that must not run
    // on the async thread.
    let handle = Arc::clone(&state);
    blocking("open_view", move || {
        let (view_id, len, generation) = handle.open_view(&parsed)?;
        Ok(ViewHandleDto { view_id, len, gen: generation })
    })
    .await
}

#[tauri::command]
pub async fn fetch_rows(
    state: State<'_, Arc<AppState>>,
    view_id: u32,
    offset: u32,
    len: u32,
) -> AppResult<Vec<RowDto>> {
    if len > MAX_ROWS {
        return Err(
            AppError::new(ErrorKind::Malformed, "Too many rows requested at once.")
                .with_detail(format!("len {len} exceeds the {MAX_ROWS}-row cap")),
        );
    }
    let library = state.library()?;
    let view = state.view(view_id)?;
    blocking("fetch_rows", move || {
        let offset = offset as usize;
        let window = view.window(offset, len as usize);
        Ok(rows_to_dto(&library, window, offset))
    })
    .await
}

#[tauri::command]
pub async fn view_ids_in_range(
    state: State<'_, Arc<AppState>>,
    view_id: u32,
    from: u32,
    to: u32,
) -> AppResult<Vec<String>> {
    let library = state.library()?;
    let view = state.view(view_id)?;
    blocking("view_ids_in_range", move || {
        Ok(library
            .ids_in_range(&view, from as usize, to as usize)
            .into_iter()
            .map(|id| id.to_string())
            .collect())
    })
    .await
}

/// Waveform bytes for a track, as raw bytes rather than JSON.
///
/// A colour waveform is a few kilobytes of numbers; sending it as a JSON array
/// would be several times larger and cost a parse on the UI thread. `tauri`
/// hands `Vec<u8>` to the webview as a binary response.
///
/// Returns an empty vector when the track has no analysis, which the UI draws
/// as a blank preview rather than an error.
#[tauri::command]
pub async fn track_waveform(
    state: State<'_, Arc<AppState>>,
    track_id: String,
    kind: String,
) -> AppResult<Vec<u8>> {
    let library = state.library()?;
    let share = state.share_root();
    let Ok(numeric) = track_id.parse::<u64>() else {
        return Err(AppError::new(ErrorKind::Malformed, "That track id is not valid.")
            .with_detail(format!("track_id {track_id:?}")));
    };

    blocking("track_waveform", move || {
        let Some(row) = library.ids.iter().position(|&id| id == numeric) else {
            return Ok(Vec::new());
        };
        let analysis_path = library.analysis_path.get(row);
        if analysis_path.is_empty() {
            return Ok(Vec::new());
        }

        // The stored path names the .DAT; the colour waveforms live in the
        // .EXT sibling and the three-band ones in .2EX.
        let dat = rbl_anlz::resolve(&share, analysis_path);
        let (file, tag): (std::path::PathBuf, [u8; 4]) = match kind.as_str() {
            "detail" => (rbl_anlz::sibling(&dat, "EXT"), *b"PWV5"),
            "colour" | "color" => (rbl_anlz::sibling(&dat, "EXT"), *b"PWV4"),
            // The overview strip above the browser.
            _ => (dat, *b"PWAV"),
        };

        let Ok(anlz) = rbl_anlz::Anlz::read(&file) else {
            return Ok(Vec::new()); // analysis missing on disk: draw nothing
        };
        Ok(anlz.waveform(&tag).map(|(_, data)| data.to_vec()).unwrap_or_default())
    })
    .await
}

/// What analysing one track produced.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResultDto {
    pub track_id: String,
    /// BPM x100, as rekordbox stores it.
    pub bpm_x100: u32,
    pub key: String,
    pub beats: u32,
    /// Peak sample magnitude, 0..=1.
    pub peak: f32,
    pub duration_sec: u32,
    pub elapsed_ms: u64,
}

/// Analyses one track and returns the result **without writing anything**.
///
/// Persisting analysis means writing to the user's library and authoring files
/// in its share tree. Neither happens until the differential recordings in
/// `docs`/`recordings` explain what rekordbox itself writes, so this reports
/// what it found and stops there.
#[tauri::command]
pub async fn analyse_track(
    state: State<'_, Arc<AppState>>,
    track_id: String,
) -> AppResult<AnalysisResultDto> {
    let library = state.library()?;
    blocking("analyse_track", move || {
        // By the id map rather than a scan: a queue analyses hundreds of
        // tracks, and each scan is 38,681 comparisons.
        let reported = track_id.clone();
        let Some(row) = library.row_of(&track_id) else {
            return Err(AppError::new(ErrorKind::NotFound, "That track is not in the library."));
        };
        let row = row as usize;
        let path = library.folder_path.get(row);
        if path.is_empty() {
            return Err(AppError::new(ErrorKind::NotFound, "That track has no file path."));
        }

        let started = std::time::Instant::now();
        // Bounded: tempo and key are global properties, and an unbounded decode
        // of a long mix is how a single track becomes a memory problem.
        let audio = rbl_audio::decode_mono(std::path::Path::new(path), Some(180.0)).map_err(|e| {
            AppError::new(ErrorKind::Malformed, "That file could not be decoded.")
                .with_detail(e.to_string())
        })?;

        let analysis = rbl_analysis::analyse(&audio.samples, audio.sample_rate);
        // Clamp before narrowing so the conversion cannot truncate. The cast is
        // safe because the value is already inside u32's range.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped into 0..=u32::MAX on the line above")]
        let to_u32 = |v: f64| {
            let clamped = v.round().clamp(0.0, f64::from(u32::MAX));
            clamped as u32
        };
        Ok(AnalysisResultDto {
            track_id: reported,
            bpm_x100: to_u32(analysis.tempo.bpm * 100.0),
            key: analysis.key.map(|k| k.name).unwrap_or_default(),
            beats: u32::try_from(analysis.tempo.beats.len()).unwrap_or(u32::MAX),
            peak: analysis.peak,
            duration_sec: to_u32(audio.duration_secs()),
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    })
    .await
}

// ---------------------------------------------------------------- editing

/// Opens the library for writing, runs one action, and reloads the index.
///
/// The writer is opened per action rather than held: holding it would keep the
/// database open read-write for the life of the app, and rekordbox launching
/// behind us must be able to take the file back. Opening is cheap next to the
/// user's own thinking time between edits.
/// What an edit changed, and therefore how much has to be re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Touched {
    /// Only the playlist tree. Re-reading it costs 24 ms against 233 ms for
    /// the whole library, and it is by far the most common kind of edit.
    Playlists,
    /// A track column changed, so the ranks and the search arena are stale.
    Tracks,
}

async fn edit<F>(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    name: &'static str,
    touched: Touched,
    action: F,
) -> AppResult<u32>
where
    F: FnOnce(&mut rbl_db::write::Writer) -> Result<(), rbl_db::DbError> + Send + 'static,
{
    let state = Arc::clone(&state);
    let changed = blocking(name, move || {
        let location = rbl_db::detect().map_err(write_error)?;
        let backups = backup_dir();
        let mut writer = rbl_db::write::Writer::open(location, backups).map_err(write_error)?;
        action(&mut writer).map_err(write_error)?;
        Ok(())
    })
    .await;
    changed?;
    match touched {
        Touched::Playlists => reload_playlists(app, state).await,
        Touched::Tracks => reload(app, state).await,
    }
}

/// Re-reads the playlist tree only, leaving the track columns in place.
async fn reload_playlists(app: tauri::AppHandle, state: Arc<AppState>) -> AppResult<u32> {
    let generation = blocking("reload_playlists", move || {
        let db = rbl_db::Library::open_installed_read_only().map_err(write_error)?;
        let library = state.library()?;
        let playlists = rbl_index::reload_playlists(&db, &library)
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
        library.set_playlists(playlists);
        // The tree changed, so every open view over a playlist is stale.
        Ok(state.invalidate_views())
    })
    .await?;
    let _ = tauri::Emitter::emit(&app, "library:changed", generation);
    Ok(generation)
}

/// Where backups of the library go before the first write of a session.
fn backup_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rekordbox-lite/backups")
}

/// Maps a database refusal onto the error kind the frontend distinguishes.
fn write_error(error: rbl_db::DbError) -> AppError {
    match error {
        rbl_db::DbError::WriteRefused(reason) => AppError::new(ErrorKind::ReadOnly, reason),
        other => AppError::new(ErrorKind::Internal, other.to_string()),
    }
}

/// Re-reads the library and returns the new generation.
async fn reload(app: tauri::AppHandle, state: Arc<AppState>) -> AppResult<u32> {
    let generation = blocking("reload", move || {
        let db = rbl_db::Library::open_installed_read_only().map_err(write_error)?;
        let db_version = db.schema().db_version;
        let share_root = db.location().share_root.clone();
        let started = std::time::Instant::now();
        let (library, _) = rbl_index::load(&db)
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
        let load_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let read_only = rbl_db::is_rekordbox_running();
        state.set_library(library, read_only, db_version, load_ms, share_root);
        Ok(state.summary().3)
    })
    .await?;
    // Cached pages are keyed on the generation, so the frontend drops them.
    let _ = tauri::Emitter::emit(&app, "library:changed", generation);
    Ok(generation)
}

/// Starts listening for devices on the link network.
///
/// Listen-only: nothing is transmitted. Announcing ourselves as a source needs
/// the database server's menus, which are not built, and a device that
/// announces and then cannot answer is worse than one that stays quiet.
#[tauri::command]
pub async fn start_link_listening(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> AppResult<LinkStatusDto> {
    if state.link_running() {
        return Ok(LinkStatusDto { listening: true, problem: None, peers: Vec::new() });
    }
    let emitter = app.clone();
    match crate::link::Listener::start(move |peers| {
        let _ = tauri::Emitter::emit(&emitter, "link:peers", peers);
    }) {
        Ok(listener) => {
            drop(state.set_link(Some(listener)));
            Ok(LinkStatusDto { listening: true, problem: None, peers: Vec::new() })
        }
        Err(e) => Ok(LinkStatusDto {
            listening: false,
            problem: Some(crate::link::explain(&e)),
            peers: Vec::new(),
        }),
    }
}

#[tauri::command]
pub async fn stop_link_listening(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    // Dropped outside the lock: the listener's Drop stops its thread.
    drop(state.set_link(None));
    Ok(())
}

/// A track's cue points.
///
/// Positions and kinds only. What colour rekordbox draws a cue is decided by
/// `djmdCue.ColorTableIndex`, which is not understood — 735,427 of the
/// reference library's cues use index 21 and nothing explains it — so no
/// colour is reported rather than a guessed one.
#[tauri::command]
pub async fn track_cues(
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<Vec<CueDto>> {
    let library = state.library()?;
    blocking("track_cues", move || {
        let Some(row) = library.row_of(&track) else { return Ok(Vec::new()) };
        Ok(library
            .cues_of(row)
            .iter()
            .map(|cue| CueDto {
                position_ms: cue.position_ms,
                letter: cue.hot_letter().map(String::from).unwrap_or_default(),
                memory: cue.is_memory(),
            })
            .collect())
    })
    .await
}

/// Tracks whose audio file is no longer where the library says it is.
///
/// Bounded rather than exhaustive: a library can have thousands missing after
/// a drive is unplugged, and a list that long is neither useful nor small
/// enough for the IPC cap. The count is exact; the list is the first page.
#[tauri::command]
pub async fn missing_tracks(
    state: State<'_, Arc<AppState>>,
    limit: u32,
) -> AppResult<MissingTracksDto> {
    let library = state.library()?;
    let wanted = (limit as usize).min(MAX_ROWS as usize);
    blocking("missing_tracks", move || {
        let mut missing = Vec::with_capacity(wanted);
        let mut total = 0_u32;
        for index in 0..library.len() {
            let path = library.folder_path.get(index);
            // An empty path is a track that never had a file, not one that
            // lost it; those are a different problem.
            if path.is_empty() || std::path::Path::new(path).exists() {
                continue;
            }
            total = total.saturating_add(1);
            if missing.len() < wanted {
                missing.push(MissingTrackDto {
                    id: library.ids.get(index).copied().unwrap_or(0).to_string(),
                    title: library.title.get(index).to_owned(),
                    artist: library.artist_name(u32::try_from(index).unwrap_or(0)).to_owned(),
                    path: path.to_owned(),
                });
            }
        }
        Ok(MissingTracksDto { total, tracks: missing })
    })
    .await
}

/// Adds files to the library.
///
/// Reports what happened per file rather than failing the whole batch: a
/// folder of a hundred tracks with two unreadable ones should import
/// ninety-eight, not nothing.
#[tauri::command]
pub async fn import_files(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> AppResult<ImportReportDto> {
    let state_for_edit = Arc::clone(&state);
    let report = blocking("import_files", move || {
        let location = rbl_db::detect().map_err(write_error)?;
        let mut writer = rbl_db::write::Writer::open(location, backup_dir()).map_err(write_error)?;
        let mut imported = 0_u32;
        let mut skipped = Vec::new();
        for path in &paths {
            match writer.import_file(std::path::Path::new(path)) {
                Ok(_) => imported += 1,
                Err(rbl_db::DbError::WriteRefused(reason)) => {
                    skipped.push(format!("{path}: {reason}"));
                }
                Err(other) => return Err(write_error(other)),
            }
        }
        Ok(ImportReportDto { imported, skipped })
    })
    .await?;

    // Only reload if anything landed; a batch that imported nothing has not
    // changed the library.
    if report.imported > 0 {
        reload(app, state_for_edit).await?;
    }
    Ok(report)
}

#[tauri::command]
pub async fn relocate_track(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    track: String,
    path: String,
) -> AppResult<u32> {
    edit(app, state, "relocate_track", Touched::Tracks, move |w| {
        w.relocate(&track, std::path::Path::new(&path)).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn create_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "create_playlist", Touched::Playlists, move |w| w.create_playlist(&name, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn create_folder(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "create_folder", Touched::Playlists, move |w| w.create_folder(&name, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn rename_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> AppResult<u32> {
    edit(app, state, "rename_playlist", Touched::Playlists, move |w| w.rename(&id, &name).map(|_| ())).await
}

#[tauri::command]
pub async fn move_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "move_playlist", Touched::Playlists, move |w| w.move_to(&id, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn delete_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> AppResult<u32> {
    edit(app, state, "delete_playlist", Touched::Playlists, move |w| w.delete_playlist(&id).map(|_| ())).await
}

#[tauri::command]
pub async fn add_tracks_to_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    tracks: Vec<String>,
) -> AppResult<u32> {
    edit(app, state, "add_tracks_to_playlist", Touched::Playlists, move |w| {
        w.add_tracks(&playlist, &tracks).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn remove_tracks_from_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    tracks: Vec<String>,
) -> AppResult<u32> {
    edit(app, state, "remove_tracks_from_playlist", Touched::Playlists, move |w| {
        w.remove_tracks(&playlist, &tracks).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn reorder_playlist(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    tracks: Vec<String>,
) -> AppResult<u32> {
    edit(app, state, "reorder_playlist", Touched::Playlists, move |w| w.reorder(&playlist, &tracks).map(|_| ())).await
}

#[tauri::command]
pub async fn set_track_rating(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    track: String,
    stars: u8,
) -> AppResult<u32> {
    edit(app, state, "set_track_rating", Touched::Tracks, move |w| w.set_rating(&track, stars).map(|_| ())).await
}

#[tauri::command]
pub async fn set_track_comment(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    track: String,
    comment: String,
) -> AppResult<u32> {
    edit(app, state, "set_track_comment", Touched::Tracks, move |w| w.set_comment(&track, &comment).map(|_| ()))
        .await
}

#[tauri::command]
pub async fn set_track_color(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    track: String,
    color: Option<String>,
) -> AppResult<u32> {
    edit(app, state, "set_track_color", Touched::Tracks, move |w| {
        w.set_color(&track, color.as_deref()).map(|_| ())
    })
    .await
}
