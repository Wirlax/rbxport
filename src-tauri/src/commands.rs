//! Tauri commands.
//!
//! Each is a thin adapter. Anything that touches the index runs on a blocking
//! thread via [`blocking`], which serves two rules at once: the async runtime
//! is never blocked, and a panic inside a command surfaces as an `AppError`
//! instead of taking the process down.

use std::sync::Arc;

use rbl_deck::{Band, Curve};
use rbl_index::Library;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::link::LinkStatusDto;
use crate::dto::{
    cue_colour_css, AudioDeviceDto, AudioDevicesDto, CueDto, DeviceDto, DeviceExportDto, ExportReportDto,
    ImportReportDto, LibrarySummaryDto, LimiterDto, MissingTrackDto, MissingTracksDto, PhraseDto, RowDto,
    TreeNodeDto, ViewHandleDto, ViewSpecDto,
    CountedDto, FilterValuesDto, TagCategoryDto,
};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::{rows_to_dto, spec_from_wire, AppState};

/// Rows per request. The frontend asks a page at a time; this bound is what
/// keeps a response inside the 64 KB cap.
pub const MAX_ROWS: u32 = 128;

/// Beats returned for one track. A four-minute track at 128 BPM has about 500
/// and a three-hour mix around 23,000; this bounds the response without
/// truncating any real grid.
const MAX_BEATS: usize = 65_536;

/// Phrases returned for one track. The longest song structure in the reference
/// library has 457 [OBS] — a two-hour DJ mix — and every ordinary track is
/// under fifty, so this bounds the response without truncating a real one.
const MAX_PHRASES: usize = 512;

/// Runs `f` on a blocking thread and converts a panic there into an `AppError`.
pub(crate) async fn blocking<T, F>(name: &'static str, f: F) -> AppResult<T>
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
    let histories = library.histories();
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

    push_lists(&mut nodes, &playlists, ListKinds { folder: "folder", leaf: "playlist" }, 2);

    // Histories only when there are some: an empty section is a heading that
    // leads nowhere, and the rail already dims what has nothing in it.
    if !histories.is_empty() {
        nodes.push(TreeNodeDto {
            id: "histories".into(),
            name: "Histories".into(),
            kind: "histories",
            depth: 0,
            // Closed. Sessions are filed under a folder per year and per month
            // and there are 187 of them in the reference library, which would
            // otherwise open over the playlists.
            expanded: Some(false),
            child_count: Some(u32::try_from(histories.len()).unwrap_or(u32::MAX)),
        });
        // A year folder and a session are both "history": they are one
        // section, and what tells them apart in the tree is whether anything
        // sits under them.
        push_lists(&mut nodes, &histories, ListKinds { folder: "history", leaf: "history" }, 0);
    }
    nodes
}

/// What to call a list with children, and one without.
#[derive(Debug, Clone, Copy)]
struct ListKinds {
    folder: &'static str,
    leaf: &'static str,
}

/// Flattens one list tree onto `nodes`, depth-first, in `Seq` order.
///
/// `open_to` is the depth below which branches arrive expanded: the tree opens
/// on the playlists, and closed on everything filed by date.
fn push_lists(
    nodes: &mut Vec<TreeNodeDto>,
    lists: &rbl_index::Playlists,
    kinds: ListKinds,
    open_to: u32,
) {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); lists.len()];
    let mut roots: Vec<usize> = Vec::new();
    for index in 0..lists.len() {
        match lists.parent.get(index).copied() {
            Some(parent) if parent != rbl_index::NO_ID && (parent as usize) < lists.len() => {
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
    let mut visited = vec![false; lists.len()];
    while let Some((index, depth)) = stack.pop() {
        if visited.get(index).copied().unwrap_or(true) {
            continue;
        }
        if let Some(slot) = visited.get_mut(index) {
            *slot = true;
        }
        let under = children.get(index).map_or(0, Vec::len);
        let members = lists.members.get(index).map_or(0, Vec::len);
        // A folder by its attribute, or by what is under it: a history year
        // is a folder only in the second sense, an empty playlist folder only
        // in the first.
        let folder = lists.is_folder(index) || under > 0;
        nodes.push(TreeNodeDto {
            id: lists.ids.get(index).copied().unwrap_or(0).to_string(),
            name: lists.name(index).to_owned(),
            kind: if folder { kinds.folder } else { kinds.leaf },
            depth,
            expanded: if folder { Some(depth < open_to) } else { None },
            child_count: Some(
                u32::try_from(if folder { under } else { members }).unwrap_or(u32::MAX),
            ),
        });
        if let Some(below) = children.get(index) {
            for &child in below.iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
}

#[tauri::command]
pub async fn open_view(state: State<'_, Arc<AppState>>, spec: ViewSpecDto) -> AppResult<ViewHandleDto> {
    let library = state.library()?;
    // A folder is read from disk, not from the index, so it takes its own
    // path before the source is translated.
    if let crate::dto::TrackSourceDto::Folder { path } = &spec.source {
        return crate::explorer::open_folder(&state, path.clone(), &spec).await;
    }
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
    if let Some(folder) = state.folder_view(view_id) {
        return crate::explorer::fetch_rows(library, folder, offset, len).await;
    }
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
    if let Some(folder) = state.folder_view(view_id) {
        return crate::explorer::ids_in_range(library, folder, from, to).await;
    }
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
/// Waveform bytes for a track, as raw bytes rather than a JSON number array.
///
/// `from` and `len` window the tag, counted in entries. Absent means the whole
/// thing, which is only safe for the small tags: `PWV7` is 158 KB on a
/// five-minute track, far past the 64 KB response cap, so the detail view asks
/// for the span it is about to draw.
pub async fn track_waveform(
    state: State<'_, Arc<AppState>>,
    track_id: String,
    kind: String,
    from: Option<u32>,
    len: Option<u32>,
) -> AppResult<tauri::ipc::Response> {
    let library = state.library()?;
    let share = state.share_root();
    let Ok(numeric) = track_id.parse::<u64>() else {
        return Err(AppError::new(ErrorKind::Malformed, "That track id is not valid.")
            .with_detail(format!("track_id {track_id:?}")));
    };

    blocking("track_waveform", move || {
        // Through the id map, not a scan. `ids` is 38,681 long and a screenful
        // of rows asks once each, which is the reason `artwork_path_of` was
        // given the map in the first place; this call was still walking the
        // whole column for every row a scroll went past.
        let Some(row) = library.row_of_id(numeric).map(|row| row as usize) else {
            return Ok(Vec::new());
        };
        let analysis_path = library.analysis_path.get(row);
        if analysis_path.is_empty() {
            return Ok(Vec::new());
        }

        // The stored path names the .DAT; the colour waveforms live in the
        // .EXT sibling and the three-band ones in .2EX.
        let dat = rbl_anlz::resolve(&share, analysis_path);
        // rekordbox 7 draws the three-band waveforms, and every one of the
        // first 300 tracks checked in the reference library has them. `PWV6`
        // is the 1,200-column overview and `PWV7` the full-resolution detail,
        // both three bytes per column: low, mid, high.
        let (file, tag, stride): (std::path::PathBuf, [u8; 4], usize) = match kind.as_str() {
            "bands" => (rbl_anlz::sibling(&dat, "2EX"), *b"PWV6", 3),
            "bandsDetail" => (rbl_anlz::sibling(&dat, "2EX"), *b"PWV7", 3),
            // The RGB palette's pair, six and two bytes a column.
            "colourDetail" | "detail" => (rbl_anlz::sibling(&dat, "EXT"), *b"PWV5", 2),
            "colour" | "color" => (rbl_anlz::sibling(&dat, "EXT"), *b"PWV4", 6),
            // The BLUE palette's pair, one byte a column.
            "monoDetail" => (rbl_anlz::sibling(&dat, "EXT"), *b"PWV3", 1),
            _ => (dat, *b"PWAV", 1),
        };

        let Ok(anlz) = rbl_anlz::Anlz::read(&file) else {
            // Analysis missing on disk: draw nothing rather than fail the view.
            return Ok(Vec::new());
        };
        let whole = anlz.waveform(&tag).map(|(_, data)| data).unwrap_or_default();
        Ok(window_of(whole, stride, from, len))
    })
    .await
    .map(tauri::ipc::Response::new)
}

/// The requested span of a waveform tag, clamped to what is there.
///
/// Entries rather than bytes, so a caller never has to know a tag's stride,
/// and so a window can never land mid-entry and shear the bands apart.
fn window_of(data: &[u8], stride: usize, from: Option<u32>, len: Option<u32>) -> Vec<u8> {
    let stride = stride.max(1);
    let entries = data.len() / stride;
    let first = from.map_or(0, |f| f as usize).min(entries);
    let count = len.map_or(entries - first, |l| (l as usize).min(entries - first));
    data.get(first * stride..(first + count) * stride).unwrap_or(&[]).to_vec()
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

/// What an edit changed, and therefore how much has to be re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touched {
    /// Only the playlist tree. Re-reading it costs 24 ms against 233 ms for
    /// the whole library, and it is by far the most common kind of edit.
    Playlists,
    /// A track column changed, so the ranks and the search arena are stale.
    Tracks,
}

/// Opens the library for writing, runs one action, and reloads the index.
///
/// The writer is opened per action rather than held (see
/// [`AppState::write`]); opening is cheap next to the user's own
/// thinking time between edits.
pub(crate) async fn edit<R: tauri::Runtime, F>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    name: &'static str,
    touched: Touched,
    action: F,
) -> AppResult<u32>
where
    F: FnOnce(&mut rbl_db::write::Writer) -> Result<(), rbl_db::DbError> + Send + 'static,
{
    let state = Arc::clone(&state);
    let writing = Arc::clone(&state);
    let changed = blocking(name, move || writing.write(action).map_err(write_error)).await;
    changed?;
    match touched {
        Touched::Playlists => reload_playlists(app, state).await,
        Touched::Tracks => reload(app, state).await,
    }
}

/// Re-reads the playlist tree only, leaving the track columns in place.
async fn reload_playlists<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: Arc<AppState>) -> AppResult<u32> {
    let generation = blocking("reload_playlists", move || {
        let db = state.open_read_only().map_err(write_error)?;
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

/// Maps a database refusal onto the error kind the frontend distinguishes.
pub(crate) fn write_error(error: rbl_db::DbError) -> AppError {
    match error {
        rbl_db::DbError::WriteRefused(reason) => AppError::new(ErrorKind::ReadOnly, reason),
        other => AppError::new(ErrorKind::Internal, other.to_string()),
    }
}

/// Re-reads the library and returns the new generation.
pub(crate) async fn reload<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: Arc<AppState>) -> AppResult<u32> {
    let generation = blocking("reload", move || {
        let db = state.open_read_only().map_err(write_error)?;
        let db_version = db.schema().db_version;
        let location = db.location().clone();
        let started = std::time::Instant::now();
        let (library, _) = rbl_index::load(&db)
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
        let load_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let read_only = rbl_db::is_rekordbox_running();
        state.set_library(library, read_only, db_version, load_ms, location);
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
pub async fn start_link_listening<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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

/// Writes a playlist to a stick.
///
/// Copies the audio, re-emits the analysis, and writes `export.pdb`,
/// `exportExt.pdb` and `exportLibrary.db`. Never re-analyses: an export moves
/// what the library already knows.
#[tauri::command]
pub async fn export_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    destination: String,
    // What a stick with no settings of its own is given; see the DJ System
    // pane. A stick that has settings keeps them.
    defaults: Option<crate::device_settings::StickDefaultsDto>,
) -> AppResult<ExportReportDto> {
    let library = state.library()?;
    let share = state.share_root();
    let report = blocking("export_playlist", move || {
        let playlists = library.playlists();
        let Some(index) = playlist
            .parse::<u64>()
            .ok()
            .and_then(|numeric| playlists.index_of(numeric))
        else {
            return Err(AppError::new(ErrorKind::NotFound, "That playlist is not in the library."));
        };
        let name = playlists.name(index).to_owned();
        let rows: Vec<u32> = playlists.members.get(index).cloned().unwrap_or_default();
        drop(playlists);

        if rows.is_empty() {
            return Err(AppError::new(
                ErrorKind::NotFound,
                "That playlist has no tracks to export.",
            ));
        }

        let mut tracks = Vec::with_capacity(rows.len());
        for &row in &rows {
            let i = row as usize;
            let analysis = read_analysis(&share, library.analysis_path.get(i));
            tracks.push(rbl_export::SourceTrack {
                // The content id is how a second export to the same stick
                // recognises a track it has already written.
                id: library.ids.get(i).copied().unwrap_or(0),
                source_path: std::path::PathBuf::from(library.folder_path.get(i)),
                title: library.title.get(i).to_owned(),
                artist: library.artist_name(row).to_owned(),
                album: library.album_name(row).to_owned(),
                genre: library.genre_name(row).to_owned(),
                label: library.label_name(row).to_owned(),
                key: library.key_name(row).to_owned(),
                comment: library.comment.get(i).to_owned(),
                date_added: library.date_added.get(i).to_owned(),
                release_date: library.release_date.get(i).to_owned(),
                bpm_x100: library.bpm_x100.get(i).copied().unwrap_or(0),
                duration_sec: u16::try_from(library.length_sec.get(i).copied().unwrap_or(0)).unwrap_or(u16::MAX),
                rating: library.rating.get(i).copied().unwrap_or(0),
                color_id: library.color.get(i).copied().unwrap_or(0),
                analysis,
                ..rbl_export::SourceTrack::default()
            });
        }

        let source_playlist = rbl_export::SourcePlaylist {
            name,
            track_indices: (0..tracks.len()).collect(),
        };
        let library_defaults = defaults.as_ref().map(crate::device_settings::library_defaults);
        let report = rbl_export::export_with(
            std::path::Path::new(&destination),
            &tracks,
            std::slice::from_ref(&source_playlist),
            library_defaults.as_ref(),
        )
        .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
        if let Some(defaults) = &defaults {
            crate::device_settings::write_dev_defaults(std::path::Path::new(&destination), defaults)?;
        }

        // Re-read what was written with the independent parser: an export that
        // cannot be read back is not an export.
        let check = rbl_export::verify(std::path::Path::new(&destination))
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;

        Ok(ExportReportDto {
            tracks: u32::try_from(report.tracks).unwrap_or(0),
            playlists: u32::try_from(report.playlists).unwrap_or(0),
            bytes_copied: report.bytes_copied,
            analysis_files: u32::try_from(report.analysis_files).unwrap_or(0),
            reused: u32::try_from(report.reused).unwrap_or(0),
            removed: u32::try_from(report.removed).unwrap_or(0),
            skipped: report.skipped,
            verified: check.parsed && check.tracks == report.tracks,
        })
    })
    .await?;

    let _ = tauri::Emitter::emit(&app, "export:done", &report);
    Ok(report)
}

/// Lists the volumes an export could be written to, and what is on each.
///
/// Enumeration is cheap; reading a stick to see what it holds is not, so that
/// happens once per device here rather than on any timer. There is no polling
/// behind this — the panel asks when it is opened.
#[tauri::command]
pub async fn list_devices() -> AppResult<Vec<DeviceDto>> {
    blocking("list_devices", || {
        Ok(rbl_devices::list()
            .into_iter()
            .map(|device| {
                let found = rbl_devices::inspect(&device.mount_point);
                DeviceDto {
                    name: device.name,
                    path: device.mount_point.to_string_lossy().into_owned(),
                    total_bytes: device.total_bytes,
                    free_bytes: device.free_bytes,
                    removable: device.removable,
                    export: found.map(|export| DeviceExportDto {
                        tracks: u32::try_from(export.tracks).unwrap_or(u32::MAX),
                        playlists: u32::try_from(export.playlists).unwrap_or(u32::MAX),
                        ours: export.ours,
                        written: export.written,
                    }),
                }
            })
            .collect())
    })
    .await
}

/// Reads a track's analysis files, so the export re-emits rather than
/// re-analysing.
fn read_analysis(share: &std::path::Path, relative: &str) -> Vec<(String, Vec<u8>)> {
    if relative.is_empty() {
        return Vec::new();
    }
    let base = share.join(relative.trim_start_matches(['/', '\\']));
    let mut out = Vec::new();
    for extension in ["DAT", "EXT"] {
        let path = base.with_extension(extension);
        if let Ok(bytes) = std::fs::read(&path) {
            out.push((extension.to_owned(), bytes));
        }
    }
    out
}

/// A track's whole beat grid, as raw bytes.
///
/// Read from the `PQTZ` tag of the track's analysis file. Five bytes a beat —
/// a little-endian `u32` of milliseconds and the beat's number in its bar —
/// so a four-minute track costs about 2.5 KB and one fetch per track replaces
/// a fetch per window. Windowing it meant re-reading and re-parsing the whole
/// analysis file every time the playhead moved on, which is the expensive part
/// whatever slice comes back.
///
/// The beat number rather than a downbeat flag: it is what the tag holds, it
/// is the same five bytes, and bar-aligned sync needs the position in the bar
/// rather than only whether the bar started.
#[tauri::command]
pub async fn track_beats(
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<tauri::ipc::Response> {
    let library = state.library()?;
    let share = state.share_root();
    blocking("track_beats", move || {
        let Some(row) = library.row_of(&track) else { return Ok(Vec::new()) };
        let relative = library.analysis_path.get(row as usize);
        if relative.is_empty() {
            return Ok(Vec::new());
        }
        let beats = read_beat_grid(&share, relative);
        let mut out: Vec<u8> = Vec::with_capacity(beats.len() * BEAT_BYTES);
        for (time_ms, number) in beats {
            out.extend_from_slice(&time_ms.to_le_bytes());
            out.push(number);
        }
        Ok(out)
    })
    .await
    .map(tauri::ipc::Response::new)
}

/// A track's beat grid from its `.DAT`: milliseconds and the beat's number
/// in the bar (1 is the downbeat), at most `MAX_BEATS` of them. Empty for a
/// track without one.
fn read_beat_grid(share: &std::path::Path, relative: &str) -> Vec<(u32, u8)> {
    let path = share.join(relative.trim_start_matches(['/', '\\']));
    let Ok(bytes) = std::fs::read(&path) else { return Vec::new() };
    let Ok(file) = rbl_anlz::parse(&bytes) else { return Vec::new() };
    file.sections
        .iter()
        .find_map(rbl_anlz::Section::as_beat_grid)
        .map(|beats| {
            beats
                .iter()
                .take(MAX_BEATS)
                .map(|beat| (beat.time_ms, u8::try_from(beat.beat_number).unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default()
}

/// Bytes one beat takes in that encoding: `u32` milliseconds, then its number.
const BEAT_BYTES: usize = 5;

/// Points a deck at a track and starts loading it.
///
/// Returns as soon as the engine has been told; the deck reports itself ready
/// with a `deck:loaded` event, because opening a file means reading from a
/// disk that may be asleep.
#[tauri::command]
pub async fn deck_load<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    track: String,
) -> AppResult<()> {
    let library = state.library()?;
    let Some(path) = library.audio_path_of(&track).map(std::path::PathBuf::from) else {
        return Err(AppError::new(ErrorKind::NotFound, "That track's file could not be found.")
            .with_detail(format!("track {track}")));
    };
    let engine = player.engine(&app)?;
    let which = crate::player::deck_of(&deck);
    // The engine's own thread does the opening; this only hands it the path.
    engine.load(which, &path);
    // And the grid, for the metronome. Read off the async thread: it is a
    // file, and the deck is loading on its own thread anyway.
    let share = state.share_root();
    let relative = library.row_of(&track).map(|row| library.analysis_path.get(row as usize).to_owned());
    let grid = blocking("deck_load_grid", move || {
        Ok(relative
            .filter(|rel| !rel.is_empty())
            .map(|rel| read_beat_grid(&share, &rel))
            .unwrap_or_default())
    })
    .await?;
    engine.set_metronome_grid(
        which,
        &grid.iter().map(|&(ms, number)| (ms, number == 1)).collect::<Vec<_>>(),
    );
    Ok(())
}

/// The key, in semitones from the track's own — a CDJ's key shift.
#[tauri::command]
pub async fn deck_key_shift<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    semitones: i8,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.set_key_shift(crate::player::deck_of(&deck), semitones);
    Ok(())
}

/// Switches a deck's metronome on or off: a click on every beat of the
/// grid while it plays.
#[tauri::command]
pub async fn deck_metronome<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    on: bool,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.set_metronome(crate::player::deck_of(&deck), on);
    Ok(())
}

/// Preferences › Audio › Metronome: which click, and how loud.
#[tauri::command]
pub async fn set_metronome(
    player: State<'_, Arc<crate::player::Player>>,
    sound: u8,
    volume: String,
) -> AppResult<()> {
    let sound = match sound {
        1 => rbl_deck::ClickSound::One,
        3 => rbl_deck::ClickSound::Three,
        _ => rbl_deck::ClickSound::Two,
    };
    let volume = match volume.as_str() {
        "small" => rbl_deck::ClickVolume::Small,
        "middle" => rbl_deck::ClickVolume::Middle,
        _ => rbl_deck::ClickVolume::Large,
    };
    player.set_metronome(sound, volume);
    Ok(())
}

/// Preferences › Audio › Sample Rate and Buffer size. Takes effect the next
/// time a deck plays, as a device change does: a stream has the rate it was
/// opened at.
#[tauri::command]
pub async fn set_audio_config(
    player: State<'_, Arc<crate::player::Player>>,
    sample_rate: Option<u32>,
    buffer_frames: Option<u32>,
) -> AppResult<()> {
    player.set_wish(rbl_deck::StreamWish { sample_rate, buffer_frames });
    Ok(())
}

#[tauri::command]
pub async fn deck_unload(
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
) -> AppResult<()> {
    if let Some(engine) = player.opened() {
        engine.unload(crate::player::deck_of(&deck));
    }
    Ok(())
}

#[tauri::command]
pub async fn deck_play<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.play(crate::player::deck_of(&deck));
    // The tick only runs while something is playing, so play is what starts it.
    crate::player::start_ticker(&app);
    Ok(())
}

/// Starts a deck after `delay_ms` of silence, counted by the audio callback:
/// quantized play on a synced deck, held for the master's next beat.
#[tauri::command]
pub async fn deck_play_after<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    delay_ms: f64,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    // Clamped to a positive number first: a delay is at most a beat, and a
    // negative or absurd one is zero rather than a wrapped count.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = if delay_ms.is_finite() && delay_ms > 0.0 {
        (delay_ms.min(60_000.0) * f64::from(engine.sample_rate()) / 1000.0).round() as u64
    } else {
        0
    };
    engine.play_after(crate::player::deck_of(&deck), frames);
    crate::player::start_ticker(&app);
    Ok(())
}

#[tauri::command]
pub async fn deck_pause(
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
) -> AppResult<()> {
    if let Some(engine) = player.opened() {
        engine.pause(crate::player::deck_of(&deck));
    }
    Ok(())
}

/// Moves a deck's playhead. Frame-exact, whatever the file's packet size.
#[tauri::command]
pub async fn deck_seek<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    // Fractional: a cue point is a place in the music, and a whole
    // millisecond is 44 frames at 44.1 kHz.
    position_ms: f64,
) -> AppResult<()> {
    if let Some(engine) = player.opened() {
        engine.seek_ms(crate::player::deck_of(&deck), position_ms);
        // So the interface sees where it landed even while paused, when no
        // tick is running.
        crate::player::start_ticker(&app);
    }
    Ok(())
}

/// The master output level, 0 to 1.
///
/// It reaches the meters on the next tick rather than coming back from here:
/// the level is the audio callback's to apply, and the interface reads what it
/// actually did rather than what it was asked for.
#[tauri::command]
pub async fn set_master_level<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    level: f32,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.master().set_gain(level);
    Ok(())
}

/// One deck's channel strip: the trim, the three bands, and the kill buttons.
///
/// Every one of these is a knob position rather than a gain in dB — what a
/// position means is the mixer's to decide, and it changes with the EQ /
/// ISOLATOR switch. The interface should not have to know the curve.
#[tauri::command]
pub async fn set_channel_band<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    band: String,
    position: f32,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    if let Some(channel) = engine.mixer().channels.get(channel_of(&deck)) {
        channel.set_band(band_of(&band), position);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_channel_kill<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    band: String,
    killed: bool,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    if let Some(channel) = engine.mixer().channels.get(channel_of(&deck)) {
        channel.set_kill(band_of(&band), killed);
    }
    Ok(())
}

/// The deck's gain, 0 to 2 — up to +6 dB, as a mixer's trim gives.
#[tauri::command]
pub async fn set_channel_trim<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    trim: f32,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    if let Some(channel) = engine.mixer().channels.get(channel_of(&deck)) {
        channel.set_trim(trim);
    }
    Ok(())
}

/// The crossfader: 0 is deck A alone, 1 is deck B alone, 0.5 is both.
#[tauri::command]
pub async fn set_crossfade<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    position: f32,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.mixer().set_crossfade(position);
    Ok(())
}

/// EQ or ISOLATOR, which is what the bottom of each band's travel means.
#[tauri::command]
pub async fn set_eq_curve<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    isolator: bool,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.mixer().set_curve(if isolator { Curve::Isolator } else { Curve::Eq });
    Ok(())
}

/// Which strip a deck name means. Anything but "b" is deck A, as everywhere.
fn channel_of(deck: &str) -> usize {
    usize::from(matches!(deck, "b" | "B"))
}

/// Which band a name means, defaulting to the one a typo cannot silence.
fn band_of(name: &str) -> Band {
    match name {
        "low" => Band::Low,
        "mid" => Band::Mid,
        _ => Band::High,
    }
}

/// How fast a deck plays, as a multiple of the file's own speed.
///
/// A ratio rather than a BPM: what BPM that comes to depends on the track, and
/// the deck does not need to know the track's to play it faster.
#[tauri::command]
pub async fn deck_tempo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    tempo: f32,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.set_tempo(crate::player::deck_of(&deck), tempo);
    Ok(())
}

/// Master Tempo: whether the pitch is held while the speed changes.
#[tauri::command]
pub async fn deck_master_tempo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    on: bool,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.set_master_tempo(crate::player::deck_of(&deck), on);
    Ok(())
}

/// The outputs the audio could go to, and which one is in use.
///
/// Read every time rather than cached: an interface is plugged in while the
/// app is open more often than not, and a list that was right at launch is a
/// list that does not have the thing somebody just connected.
#[tauri::command]
pub async fn audio_devices(
    player: State<'_, Arc<crate::player::Player>>,
) -> AppResult<AudioDevicesDto> {
    let chosen = player.device();
    Ok(AudioDevicesDto {
        devices: rbl_deck::output_devices()
            .into_iter()
            .map(|device| AudioDeviceDto { id: device.id, name: device.name })
            .collect(),
        default: rbl_deck::default_output_device().map(|device| device.id),
        chosen,
    })
}

/// Chooses an output. `None` — an absent id — is the system default.
///
/// It takes effect on the next thing played: a running stream belongs to the
/// device it was opened on, so the engine is dropped and rebuilt rather than
/// moved.
#[tauri::command]
pub async fn set_audio_device(
    player: State<'_, Arc<crate::player::Player>>,
    device: Option<String>,
) -> AppResult<()> {
    player.set_device(device);
    Ok(())
}

/// The master limiter as it stands.
#[tauri::command]
pub async fn master_limiter(
    player: State<'_, Arc<crate::player::Player>>,
) -> AppResult<LimiterDto> {
    Ok(player.limiter())
}

/// Sets the master limiter and returns what was actually set.
///
/// Takes effect at once if the engine is up, and is remembered for its build
/// if not — or its rebuild, after a device change. What comes back is the
/// engine's clamping of what was sent, so the interface shows the truth.
#[tauri::command]
pub async fn set_master_limiter(
    player: State<'_, Arc<crate::player::Player>>,
    limiter: LimiterDto,
) -> AppResult<LimiterDto> {
    Ok(player.set_limiter(limiter))
}

/// Shows a track's file in the Finder.
///
/// The OS does the revealing; this only resolves the id to the path the
/// library holds for it, and says so plainly when that file is not there —
/// about one track in thirty of the reference library sits on a volume that
/// is not mounted.
#[tauri::command]
pub async fn reveal_track<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<()> {
    let library = state.library()?;
    let Some(path) = library.audio_path_of(&track).map(std::path::PathBuf::from) else {
        return Err(AppError::new(ErrorKind::NotFound, "That track's file could not be found.")
            .with_detail(format!("track {track}")));
    };
    if !path.exists() {
        return Err(AppError::new(ErrorKind::NotFound, "That track's file could not be found.")
            .with_detail(path.display().to_string()));
    }
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        AppError::new(ErrorKind::Internal, "The Finder would not open.").with_detail(e.to_string())
    })
}

/// What the app is costing right now, for the title bar's readout.
///
/// Sampled on demand: nothing keeps this up to date in the background, so a
/// window with the readout hidden pays nothing for it.
/// The version this build carries — the tag it was built from, or 0.1.0
/// for a development build — for the About pane. Nothing is asked over the
/// network; that is the Update Manager's job.
#[tauri::command]
pub async fn app_version<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<String> {
    Ok(app.package_info().version.to_string())
}

#[tauri::command]
pub async fn app_diagnostics() -> AppResult<crate::diagnostics::Diagnostics> {
    // The sampler is kept between calls: CPU is a difference between two
    // readings, and a fresh `System` every second would always report zero.
    blocking("app_diagnostics", || Ok(crate::diagnostics::sample_shared())).await
}

/// Starts a drag on a deck.
///
/// Audio follows the pointer from here until `deck_scrub_end`: the deck reads
/// a decoded window at whatever rate the drag asks for, forwards or backwards,
/// which is what a hand on a record does and what a seek per pointer move
/// cannot do.
#[tauri::command]
pub async fn deck_scrub_begin<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
) -> AppResult<()> {
    let engine = player.engine(&app)?;
    engine.scrub_begin(crate::player::deck_of(&deck));
    crate::player::start_ticker(&app);
    Ok(())
}

/// Where the pointer is now, mid-drag.
#[tauri::command]
pub async fn deck_scrub_to(
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
    // Fractional: the head's speed comes from how far this moved since the
    // last one, so rounding it to a millisecond quantises the speed.
    position_ms: f64,
) -> AppResult<()> {
    if let Some(engine) = player.opened() {
        engine.scrub_to_ms(crate::player::deck_of(&deck), position_ms);
    }
    Ok(())
}

/// Ends a drag. The playhead stays where the head came to rest.
#[tauri::command]
pub async fn deck_scrub_end<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    player: State<'_, Arc<crate::player::Player>>,
    deck: String,
) -> AppResult<()> {
    if let Some(engine) = player.opened() {
        engine.scrub_end(crate::player::deck_of(&deck));
        crate::player::start_ticker(&app);
    }
    Ok(())
}

/// Both decks now, for the interface to anchor itself when it starts up or
/// comes back from a reload.
#[tauri::command]
pub async fn deck_state(
    player: State<'_, Arc<crate::player::Player>>,
) -> AppResult<crate::player::TickDto> {
    Ok(player.opened().map_or_else(crate::player::TickDto::silent, |engine| {
        crate::player::tick_of(&engine.snapshot(), engine.master())
    }))
}

/// A track's cue points.
///
/// A hot cue's colour is what rekordbox paints for its `ColorTableIndex`,
/// from the nine indices measured in `rbl_anlz::DRAWN_CUE_COLOURS`; an index
/// outside those is reported without a colour, and the interface draws its
/// default green rather than a guess. A memory cue never carries one.
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
                id: if cue.id == 0 { String::new() } else { cue.id.to_string() },
                position_ms: cue.position_ms,
                out_ms: cue.out_ms,
                letter: cue.hot_letter().map(String::from).unwrap_or_default(),
                memory: cue.is_memory(),
                colour: if cue.is_memory() { None } else { cue_colour_css(cue.colour) },
            })
            .collect())
    })
    .await
}

/// A track's phrases: the `INTRO` / `UP` / `CHORUS` strip rekordbox draws
/// above the waveform.
///
/// From `PSSI` in the `.EXT` file, with each phrase's beat resolved against
/// the `PQTZ` grid in the `.DAT` so the strip can be drawn on a time axis
/// without the caller fetching the grid as well.
#[tauri::command]
pub async fn track_phrases(
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<Vec<PhraseDto>> {
    let library = state.library()?;
    let share = state.share_root();
    blocking("track_phrases", move || {
        let Some(row) = library.row_of(&track) else { return Ok(Vec::new()) };
        let relative = library.analysis_path.get(row as usize);
        if relative.is_empty() {
            return Ok(Vec::new());
        }
        let dat = rbl_anlz::resolve(&share, relative);

        let Ok(ext) = rbl_anlz::Anlz::read(&rbl_anlz::sibling(&dat, "EXT")) else {
            // Not analysed for phrases, or the file is gone: draw no strip
            // rather than fail the view.
            return Ok(Vec::new());
        };
        let Some(phrases) = ext.phrases() else { return Ok(Vec::new()) };

        // The grid is optional here. A phrase without a time is still worth
        // returning, since its beat number is what the tag actually holds.
        let grid = rbl_anlz::Anlz::read(&dat).ok().and_then(|d| d.beat_grid());

        Ok(phrases
            .into_iter()
            .take(MAX_PHRASES)
            .map(|phrase| PhraseDto {
                beat: u32::from(phrase.beat),
                label: phrase.label.to_owned(),
                kind: phrase.kind,
                // Beat numbers in `PSSI` are 1-based; the grid is a list.
                time_ms: grid.as_ref().and_then(|g| {
                    g.get(usize::from(phrase.beat).checked_sub(1)?).map(|b| b.time_ms)
                }),
            })
            .collect())
    })
    .await
}

/// Where rekordbox heard a voice, one intensity byte per 46.44 ms.
///
/// From `PVDI` in the `.2EX` file. Raw bytes rather than JSON: a long track
/// has tens of thousands of them, and `from` / `len` window the strip the same
/// way [`track_waveform`] windows a waveform, which is what keeps a response
/// inside the IPC cap.
#[tauri::command]
pub async fn track_vocals(
    state: State<'_, Arc<AppState>>,
    track: String,
    from: Option<u32>,
    len: Option<u32>,
) -> AppResult<tauri::ipc::Response> {
    let library = state.library()?;
    let share = state.share_root();
    blocking("track_vocals", move || {
        let Some(row) = library.row_of(&track) else { return Ok(Vec::new()) };
        let relative = library.analysis_path.get(row as usize);
        if relative.is_empty() {
            return Ok(Vec::new());
        }
        let dat = rbl_anlz::resolve(&share, relative);
        let Ok(two) = rbl_anlz::Anlz::read(&rbl_anlz::sibling(&dat, "2EX")) else {
            return Ok(Vec::new());
        };
        Ok(window_of(two.vocals().unwrap_or_default(), 1, from, len))
    })
    .await
    .map(tauri::ipc::Response::new)
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
pub async fn import_files<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> AppResult<ImportReportDto> {
    let state_for_edit = Arc::clone(&state);
    let writing = Arc::clone(&state);
    let report = blocking("import_files", move || {
        writing
            .write(|writer| {
                let mut imported = 0_u32;
                let mut skipped = Vec::new();
                let mut tracks = Vec::new();
                for path in &paths {
                    let file = std::path::Path::new(path);
                    match writer.import_file(file) {
                        Ok(id) => {
                            imported += 1;
                            tracks.push(crate::dto::ImportedTrackDto {
                                id,
                                title: file
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            });
                        }
                        Err(rbl_db::DbError::WriteRefused(reason)) => {
                            skipped.push(format!("{path}: {reason}"));
                        }
                        Err(other) => return Err(other),
                    }
                }
                Ok(ImportReportDto { imported, skipped, tracks })
            })
            .map_err(write_error)
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
pub async fn relocate_track<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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
pub async fn create_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    name: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "create_playlist", Touched::Playlists, move |w| w.create_playlist(&name, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn create_folder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    name: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "create_folder", Touched::Playlists, move |w| w.create_folder(&name, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn rename_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> AppResult<u32> {
    edit(app, state, "rename_playlist", Touched::Playlists, move |w| w.rename(&id, &name).map(|_| ())).await
}

#[tauri::command]
pub async fn move_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
    parent: String,
) -> AppResult<u32> {
    edit(app, state, "move_playlist", Touched::Playlists, move |w| w.move_to(&id, &parent).map(|_| ())).await
}

#[tauri::command]
pub async fn delete_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> AppResult<u32> {
    edit(app, state, "delete_playlist", Touched::Playlists, move |w| w.delete_playlist(&id).map(|_| ())).await
}

#[tauri::command]
pub async fn add_tracks_to_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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
pub async fn remove_tracks_from_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
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
pub async fn reorder_playlist<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    tracks: Vec<String>,
) -> AppResult<u32> {
    edit(app, state, "reorder_playlist", Touched::Playlists, move |w| w.reorder(&playlist, &tracks).map(|_| ())).await
}

#[tauri::command]
pub async fn set_track_rating<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
    stars: u8,
) -> AppResult<u32> {
    edit(app, state, "set_track_rating", Touched::Tracks, move |w| w.set_rating(&track, stars).map(|_| ())).await
}

#[tauri::command]
pub async fn set_track_comment<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
    comment: String,
) -> AppResult<u32> {
    edit(app, state, "set_track_comment", Touched::Tracks, move |w| w.set_comment(&track, &comment).map(|_| ()))
        .await
}

#[tauri::command]
pub async fn set_track_color<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
    color: Option<String>,
) -> AppResult<u32> {
    edit(app, state, "set_track_color", Touched::Tracks, move |w| {
        w.set_color(&track, color.as_deref()).map(|_| ())
    })
    .await
}

/// The BPMs and keys the track filter bar can offer for a list.
///
/// Counted over the source and query alone — never over the filter's own
/// result, or a picked value would hide the others. One pass over the rows
/// in Rust; the frontend never scans a row array for this.
#[tauri::command]
pub async fn filter_values(
    state: State<'_, Arc<AppState>>,
    spec: ViewSpecDto,
) -> AppResult<FilterValuesDto> {
    let library = state.library()?;
    let parsed = spec_from_wire(&library, &spec);
    blocking("filter_values", move || {
        let values = library.filter_values(&parsed);
        Ok(FilterValuesDto {
            bpms: values
                .bpms
                .into_iter()
                .map(|c| CountedDto { value: c.value, count: c.count })
                .collect(),
            keys: values
                .keys
                .into_iter()
                .map(|c| CountedDto { value: c.value, count: c.count })
                .collect(),
            tags: values
                .tags
                .into_iter()
                .map(|c| TagCategoryDto { name: c.name, tags: c.tags })
                .collect(),
        })
    })
    .await
}
