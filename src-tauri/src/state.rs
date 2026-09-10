//! Application state: the loaded library and the open views.
//!
//! The library is loaded once and shared immutably. Views are `Vec<Row>` held
//! here rather than sent to the frontend, which is what keeps IPC payloads to a
//! window of rows regardless of collection size.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use rbl_index::{BpmFilter, Library, SortColumn, TrackFilter, TrackSource, View, ViewSpec, COLOR_NAMES};

use crate::dto::{RowDto, TrackFilterDto, TrackSourceDto, ViewSpecDto};
use crate::error::{AppError, AppResult, ErrorKind};

/// Views are dropped oldest-first past this many, so a user clicking through
/// playlists cannot grow memory without bound.
const MAX_VIEWS: usize = 16;

pub struct AppState {
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    library: Option<Arc<Library>>,
    /// Root of rekordbox's share tree, where analysis files live.
    share_root: std::path::PathBuf,
    read_only: bool,
    db_version: Option<i64>,
    load_ms: u64,
    views: HashMap<u32, Arc<View>>,
    /// Insertion order, for eviction.
    view_order: Vec<u32>,
    next_view_id: u32,
    /// Bumped when the library is reloaded; invalidates cached pages.
    generation: u32,
    /// The Pro DJ Link listener, while one is running.
    link: Option<crate::link::Listener>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self { inner: RwLock::new(Inner { next_view_id: 1, generation: 1, ..Inner::default() }) }
    }

    pub fn set_library(
        &self,
        library: Library,
        read_only: bool,
        db_version: Option<i64>,
        load_ms: u64,
        share_root: std::path::PathBuf,
    ) {
        let mut inner = self.inner.write();
        inner.library = Some(Arc::new(library));
        inner.share_root = share_root;
        inner.read_only = read_only;
        inner.db_version = db_version;
        inner.load_ms = load_ms;
        inner.views.clear();
        inner.view_order.clear();
        inner.generation = inner.generation.wrapping_add(1).max(1);
    }

    /// Drops every open view and bumps the generation.
    ///
    /// Used after a playlist edit: the track columns are untouched, but a view
    /// over a playlist whose membership changed is stale, and so are the pages
    /// the frontend has cached against the old generation.
    pub fn invalidate_views(&self) -> u32 {
        let mut inner = self.inner.write();
        inner.views.clear();
        inner.view_order.clear();
        inner.generation = inner.generation.wrapping_add(1).max(1);
        inner.generation
    }

    /// Starts or replaces the link listener. Returns the previous one, if any,
    /// so the caller can drop it outside the lock.
    pub fn set_link(&self, listener: Option<crate::link::Listener>) -> Option<crate::link::Listener> {
        std::mem::replace(&mut self.inner.write().link, listener)
    }

    pub fn link_running(&self) -> bool {
        self.inner.read().link.is_some()
    }

    pub fn library(&self) -> AppResult<Arc<Library>> {
        self.inner
            .read()
            .library
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "The library has not finished loading yet."))
    }

    /// Where analysis files live for the loaded library.
    pub fn share_root(&self) -> std::path::PathBuf {
        self.inner.read().share_root.clone()
    }

    pub fn summary(&self) -> (bool, Option<i64>, u64, u32) {
        let inner = self.inner.read();
        (inner.read_only, inner.db_version, inner.load_ms, inner.generation)
    }

    /// Opens a view and returns `(view_id, len, generation)`.
    pub fn open_view(&self, spec: &ViewSpec) -> AppResult<(u32, u32, u32)> {
        let library = self.library()?;
        let view = library.open_view(spec);
        let len = u32::try_from(view.len()).unwrap_or(u32::MAX);

        let mut inner = self.inner.write();
        let id = inner.next_view_id;
        inner.next_view_id = inner.next_view_id.wrapping_add(1).max(1);
        inner.views.insert(id, Arc::new(view));
        inner.view_order.push(id);
        while inner.view_order.len() > MAX_VIEWS {
            let oldest = inner.view_order.remove(0);
            inner.views.remove(&oldest);
        }
        let generation = inner.generation;
        Ok((id, len, generation))
    }

    pub fn view(&self, view_id: u32) -> AppResult<Arc<View>> {
        self.inner.read().views.get(&view_id).cloned().ok_or_else(|| {
            AppError::new(ErrorKind::NotFound, "That list is no longer open. Reselect it to continue.")
                .with_detail(format!("view {view_id} was evicted or never existed"))
        })
    }
}

/// Translates a wire sort name. Unknown names fall back to track order rather
/// than failing the whole request.
pub fn sort_from_wire(name: &str) -> SortColumn {
    match name {
        "title" => SortColumn::Title,
        "artist" => SortColumn::Artist,
        "album" => SortColumn::Album,
        "genre" => SortColumn::Genre,
        "label" => SortColumn::Label,
        "key" => SortColumn::Key,
        "bpm" => SortColumn::Bpm,
        "duration" => SortColumn::Duration,
        "rating" => SortColumn::Rating,
        "dateAdded" => SortColumn::DateAdded,
        "releaseDate" => SortColumn::ReleaseDate,
        _ => SortColumn::TrackNo,
    }
}

pub fn spec_from_wire(library: &Library, dto: &ViewSpecDto) -> ViewSpec {
    // An id that names nothing falls back to the collection rather than
    // erroring: a tree node can outlive what it points at, and a window of the
    // whole library is a better answer to that than a red bar.
    let source = match &dto.source {
        TrackSourceDto::Collection => TrackSource::Collection,
        TrackSourceDto::History { id } => id
            .parse::<u64>()
            .ok()
            .and_then(|numeric| library.histories().index_of(numeric))
            .map_or(TrackSource::Collection, TrackSource::History),
        TrackSourceDto::Playlist { id } => id
            .parse::<u64>()
            .ok()
            .and_then(|numeric| library.playlists().index_of(numeric))
            .map_or(TrackSource::Collection, TrackSource::Playlist),
    };
    ViewSpec {
        source,
        sort: sort_from_wire(&dto.sort),
        descending: dto.descending,
        query: dto.query.clone(),
        filter: filter_from_wire(library, &dto.filter),
    }
}

/// Translates the filter bar's picks, resolving names to ids.
///
/// A key name the library does not hold resolves to no id, so a ticked key
/// column with only unknown names matches nothing — which is what the list
/// would show for it. A colour name outside the eight is dropped the same way.
pub fn filter_from_wire(library: &Library, dto: &TrackFilterDto) -> TrackFilter {
    TrackFilter {
        bpm: dto.bpm.as_ref().map(|b| BpmFilter {
            values: b.values.clone(),
            tolerance_pct: b.tolerance_pct.min(6),
            master_bpm_x100: b.master_bpm_x100.filter(|&bpm| bpm > 0),
        }),
        keys: dto.keys.as_ref().map(|names| library.key_ids_named(names)),
        ratings: dto.ratings.clone(),
        colors: dto.colors.as_ref().map(|names| {
            names
                .iter()
                .filter_map(|name| {
                    COLOR_NAMES
                        .iter()
                        .position(|&c| c == name)
                        .and_then(|at| u8::try_from(at + 1).ok())
                })
                .collect()
        }),
    }
}

/// Builds the wire rows for a window. `position` is the row's 1-based place in
/// the view, which is what the `#` column shows.
pub fn rows_to_dto(library: &Library, rows: &[rbl_index::Row], first_position: usize) -> Vec<RowDto> {
    rows.iter()
        .enumerate()
        .map(|(offset, &row)| {
            let index = row as usize;
            RowDto {
                id: library.ids.get(index).copied().unwrap_or(0).to_string(),
                track_no: u32::try_from(first_position + offset + 1).unwrap_or(u32::MAX),
                title: library.title.get(index).to_owned(),
                artist: library.artist_name(row).to_owned(),
                album: library.album_name(row).to_owned(),
                genre: library.genre_name(row).to_owned(),
                label: library.label_name(row).to_owned(),
                comment: library.comment.get(index).to_owned(),
                bpm_x100: library.bpm_x100.get(index).copied().unwrap_or(0),
                key: library.key_name(row).to_owned(),
                duration_sec: library.length_sec.get(index).copied().unwrap_or(0),
                rating: library.rating.get(index).copied().unwrap_or(0),
                analysed: library.analysed.get(index).copied().unwrap_or(0),
                date_added: library.date_added.get(index).to_owned(),
                release_date: library.release_date.get(index).to_owned(),
                cues: String::new(),
                // Stable per track so the placeholder tint does not flicker on scroll.
                has_artwork: !library.artwork_path.get(index).is_empty(),
                artwork_hue: u16::try_from(
                    library.ids.get(index).copied().unwrap_or(0) % 360,
                )
                .unwrap_or(0),
            }
        })
        .collect()
}
