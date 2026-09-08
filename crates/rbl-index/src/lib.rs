//! In-memory columnar index over the rekordbox library.
//!
//! The whole library is loaded once into struct-of-arrays form and every list
//! operation — sorting, filtering, searching — happens here. The UI only ever
//! receives a window of rows, so a 38k-track library costs the same to browse
//! as a 100-track one.
//!
//! Layout choices that matter for the budgets:
//! - Strings live in packed arenas, not `Vec<String>` (see [`strings`]).
//! - Lookup columns (artist, album, genre, label, key) are `u32` ids into
//!   interners, so a sort compares small integers or pre-folded strings.
//! - Sort order is precomputed per column as a rank array, which turns a sort
//!   into `sort_unstable_by_key` over `u32`s.

pub mod strings;
pub mod testing;
mod load;
mod view;

pub use load::{load, reload_playlists, LoadStats};
pub use view::{SortColumn, TrackSource, View, ViewSpec};

use strings::{Interner, StrColumn};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Row index within a snapshot. Not stable across reloads.
pub type Row = u32;

/// Sentinel for "no lookup value", matching a NULL foreign key.
pub const NO_ID: u32 = u32::MAX;

/// The library, as columns.
#[derive(Debug, Default)]
pub struct Library {
    pub(crate) count: usize,

    /// `djmdContent.ID` parsed to u64; the display id is the decimal string.
    pub ids: Vec<u64>,
    pub title: StrColumn,
    pub title_folded: StrColumn,
    pub comment: StrColumn,
    pub folder_path: StrColumn,
    pub file_name: StrColumn,
    pub analysis_path: StrColumn,
    /// `djmdContent.ImagePath`, share-relative. Empty for the roughly half of
    /// the library with no artwork.
    pub artwork_path: StrColumn,
    pub date_added: StrColumn,
    pub release_date: StrColumn,

    pub artist: Vec<u32>,
    pub album: Vec<u32>,
    pub genre: Vec<u32>,
    pub label: Vec<u32>,
    pub key: Vec<u32>,

    pub bpm_x100: Vec<u32>,
    pub length_sec: Vec<u32>,
    pub rating: Vec<u8>,
    pub color: Vec<u8>,
    pub play_count: Vec<u16>,
    pub analysed: Vec<u8>,

    pub artists: Interner,
    pub albums: Interner,
    pub genres: Interner,
    pub labels: Interner,
    pub keys: Interner,

    /// The playlist tree.
    ///
    /// Behind a lock because it is the one part of the library an edit can
    /// change without touching the track columns: rebuilding it costs 24 ms
    /// against 233 ms for a full reload, and a playlist edit is by far the
    /// most common one.
    playlists: RwLock<Playlists>,

    /// Row index by track id. Built on first lookup, not at load.
    by_id: OnceLock<HashMap<u64, Row>>,

    /// Per-column collation ranks; `ranks[col][row]` orders rows without
    /// touching strings during a sort.
    pub(crate) ranks: Vec<Vec<u32>>,

    /// One folded haystack per row: title, artist, album, comment.
    pub(crate) search: StrColumn,
}

#[derive(Debug, Default, Clone)]
pub struct Playlists {
    pub ids: Vec<u64>,
    pub names: StrColumn,
    pub parent: Vec<u32>,
    pub seq: Vec<u32>,
    /// Row indices per playlist, in `TrackNo` order.
    pub members: Vec<Vec<Row>>,
}

impl Playlists {
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
    pub fn name(&self, index: usize) -> &str {
        self.names.get(index)
    }
    /// Index of a playlist by its rekordbox id.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|&x| x == id)
    }
}

impl Library {
    /// The share-relative artwork path for a track's display id, if it has one.
    ///
    /// Backed by a map built on first use. Scanning `ids` instead would be
    /// 38,681 comparisons per row, and a screenful of rows each ask once.
    /// Built lazily so a session that never shows artwork never pays for it.
    pub fn artwork_path_of(&self, display_id: &str) -> Option<&str> {
        let wanted: u64 = display_id.parse().ok()?;
        let row = *self.row_by_id().get(&wanted)?;
        Some(self.artwork_path.get(row as usize))
    }

    /// Row index by track id, built once.
    fn row_by_id(&self) -> &HashMap<u64, Row> {
        self.by_id.get_or_init(|| {
            let mut map = HashMap::with_capacity(self.ids.len());
            for (row, id) in self.ids.iter().enumerate() {
                map.insert(*id, u32::try_from(row).unwrap_or(u32::MAX));
            }
            map
        })
    }

    /// Reads the playlist tree. The guard is held only for the read.
    pub fn playlists(&self) -> parking_lot::RwLockReadGuard<'_, Playlists> {
        self.playlists.read()
    }

    /// Swaps in a freshly-read playlist tree, leaving the track columns alone.
    pub fn set_playlists(&self, playlists: Playlists) {
        *self.playlists.write() = playlists;
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    pub fn artist_name(&self, row: Row) -> &str {
        self.artists.name(self.artist.get(row as usize).copied().unwrap_or(NO_ID))
    }
    #[inline]
    pub fn album_name(&self, row: Row) -> &str {
        self.albums.name(self.album.get(row as usize).copied().unwrap_or(NO_ID))
    }
    #[inline]
    pub fn genre_name(&self, row: Row) -> &str {
        self.genres.name(self.genre.get(row as usize).copied().unwrap_or(NO_ID))
    }
    #[inline]
    pub fn label_name(&self, row: Row) -> &str {
        self.labels.name(self.label.get(row as usize).copied().unwrap_or(NO_ID))
    }
    #[inline]
    pub fn key_name(&self, row: Row) -> &str {
        self.keys.name(self.key.get(row as usize).copied().unwrap_or(NO_ID))
    }

    /// Approximate heap footprint, for the memory budget.
    pub fn heap_bytes(&self) -> usize {
        let vecs = self.ids.capacity() * 8
            + (self.artist.capacity() + self.album.capacity() + self.genre.capacity()
                + self.label.capacity() + self.key.capacity()
                + self.bpm_x100.capacity() + self.length_sec.capacity()
                + self.play_count.capacity()) * 4
            + self.rating.capacity() + self.color.capacity() + self.analysed.capacity();
        let strings = self.title.heap_bytes() + self.title_folded.heap_bytes()
            + self.comment.heap_bytes() + self.folder_path.heap_bytes()
            + self.file_name.heap_bytes() + self.analysis_path.heap_bytes()
            + self.artwork_path.heap_bytes()
            + self.date_added.heap_bytes() + self.release_date.heap_bytes()
            + self.search.heap_bytes();
        let interners = self.artists.heap_bytes() + self.albums.heap_bytes()
            + self.genres.heap_bytes() + self.labels.heap_bytes() + self.keys.heap_bytes();
        let ranks: usize = self.ranks.iter().map(|r| r.capacity() * 4).sum();
        let playlists = self.playlists().ids.capacity() * 8
            + self.playlists().names.heap_bytes()
            + self.playlists().members.iter().map(|m| m.capacity() * 4).sum::<usize>();
        vecs + strings + interners + ranks + playlists
    }
}
