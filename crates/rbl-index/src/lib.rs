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

pub mod cache;
pub mod strings;
pub mod testing;
mod filter;
mod load;
mod view;

pub use filter::{
    whole_bpm, BpmFilter, Counted, FilterValues, TagCategory, TrackFilter, COLOR_NAMES,
};
pub use load::{content_version, load, reload_cues_of, reload_playlists, LoadStats};
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
    /// The history tree, which is the same shape and read the same way.
    histories: RwLock<Playlists>,

    /// Row index by track id. Built on first lookup, not at load.
    by_id: OnceLock<HashMap<u64, Row>>,

    /// Every cue, grouped by track.
    ///
    /// Behind a lock for the same reason the playlists are: a cue edit changes
    /// one track's cues and nothing else, and re-reading one track's rows
    /// costs 0.6 ms on the reference library [OBS] — `djmdCue` is indexed on
    /// `(ContentID, rb_local_deleted)` — against 233 ms for a full reload.
    cues: RwLock<Cues>,

    /// Per-column collation ranks; `ranks[col][row]` orders rows without
    /// touching strings during a sort.
    pub(crate) ranks: Vec<Vec<u32>>,

    /// One folded haystack per row: title, artist, album, comment.
    pub(crate) search: StrColumn,

    /// The My Tag categories and their tags, by name only.
    ///
    /// `djmdMyTag` is 181 rows on the reference library (99 live), read so
    /// the filter bar can head its tag columns the way rekordbox does. Which
    /// tracks carry which tag — `djmdSongMyTag` — is **not** read: it holds no
    /// rows at all on the reference library, so what it would cost on a
    /// tagged one is `[UNKNOWN]`, and the tag columns stay inert until a
    /// library with tags in it has been measured.
    pub(crate) my_tags: Vec<TagCategory>,
}

/// One cue point.
///
/// `Kind` 0 is a memory cue; 1, 2, 3 and 5 are hot cues A to D, 6 to 9 are E
/// to H, and 10 to 17 are I to P — rekordbox 7 has sixteen. Kind 4 is unused,
/// which is why D is 5. Counted across all 1,040,598 cues in the reference
/// library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cue {
    /// `djmdCue.ID` parsed. Every id in the reference library is a decimal
    /// under 2^32 [OBS], so a `u32` holds them all; one that does not parse
    /// is kept as 0, which the interface treats as a cue it cannot edit.
    pub id: u32,
    /// Milliseconds from the start of the track.
    pub position_ms: u32,
    /// Where a loop ends, or 0 for a plain cue. `OutMsec` is -1 or NULL on
    /// every cue that is not a loop [OBS], and a loop cannot end at 0.
    pub out_ms: u32,
    /// `djmdCue.Kind`, raw. Use [`Cue::hot_letter`] to read it.
    pub kind: u8,
}

/// Every cue, grouped by track and ordered by position within each.
#[derive(Debug, Default)]
pub struct Cues {
    cues: Vec<Cue>,
    /// Where each track's cues start in `cues`; one longer than the track
    /// count, so a track's slice is `[index[row], index[row + 1])`. Empty
    /// until the first track's cues are set, and `of` reads that as none.
    index: Vec<u32>,
}

impl Cues {
    /// Builds the table from one list per track, in row order.
    pub(crate) fn from_per_track(mut per_track: Vec<Vec<Cue>>) -> Self {
        let mut index = Vec::with_capacity(per_track.len() + 1);
        let mut cues = Vec::with_capacity(per_track.iter().map(Vec::len).sum());
        for list in &mut per_track {
            index.push(u32::try_from(cues.len()).unwrap_or(u32::MAX));
            list.sort_by_key(|c| (c.position_ms, c.kind));
            cues.append(list);
        }
        // One past the end, so the last track's slice has a bound.
        index.push(u32::try_from(cues.len()).unwrap_or(u32::MAX));
        Self { cues, index }
    }

    /// A track's cues, ordered by position.
    pub fn of(&self, row: Row) -> &[Cue] {
        let start = self.index.get(row as usize).copied().unwrap_or(0) as usize;
        let end = self.index.get(row as usize + 1).copied().unwrap_or(0) as usize;
        self.cues.get(start..end).unwrap_or(&[])
    }

    /// Replaces one track's cues, leaving every other track's where they are.
    ///
    /// A splice rather than a rebuild: the tail moves by the difference in
    /// length, which for 308,628 cues of 16 bytes is a memmove of at most 5
    /// MB — well under a millisecond, and far less than re-reading the table.
    /// `tracks` sizes the index the first time a library built without cues
    /// gets one.
    pub fn replace(&mut self, row: Row, tracks: usize, mut cues: Vec<Cue>) {
        if self.index.len() < tracks + 1 {
            let end = u32::try_from(self.cues.len()).unwrap_or(u32::MAX);
            self.index.resize(tracks + 1, end);
        }
        let Some(&start) = self.index.get(row as usize) else { return };
        let Some(&end) = self.index.get(row as usize + 1) else { return };
        let (start, end) = (start as usize, end as usize);
        if end < start || end > self.cues.len() {
            return;
        }
        cues.sort_by_key(|c| (c.position_ms, c.kind));
        let grew = i64::try_from(cues.len()).unwrap_or(0) - i64::try_from(end - start).unwrap_or(0);
        self.cues.splice(start..end, cues);
        for later in self.index.iter_mut().skip(row as usize + 1) {
            let shifted = i64::from(*later) + grew;
            *later = u32::try_from(shifted).unwrap_or(u32::MAX);
        }
    }

    pub fn len(&self) -> usize {
        self.cues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
}

impl Cue {
    /// `None` for a memory cue, otherwise `A` to `P`.
    #[must_use]
    pub fn hot_letter(&self) -> Option<char> {
        // 1,2,3 then 5.. — kind 4 is not used, so D is 5 and the run is
        // contiguous from there.
        // Kind 0 is a memory cue and 4 is unused; both fall through to None
        // for different reasons, which is why they are not one arm.
        let slot = match self.kind {
            1..=3 => u32::from(self.kind) - 1,
            5..=17 => u32::from(self.kind) - 2,
            _ => return None,
        };
        char::from_u32(u32::from(b'A') + slot)
    }

    #[must_use]
    pub const fn is_memory(&self) -> bool {
        self.kind == 0
    }

    /// The `Kind` a hot-cue letter is stored as: the inverse of
    /// [`Cue::hot_letter`]. `None` for anything past `P`, or not a letter.
    #[must_use]
    pub fn kind_of_letter(letter: char) -> Option<u8> {
        let slot = u8::try_from(u32::from(letter.to_ascii_uppercase()).checked_sub(u32::from(b'A'))?).ok()?;
        match slot {
            // A to C are 1 to 3; 4 is unused, so D and everything after it
            // sit one higher.
            0..=2 => Some(slot + 1),
            3..=15 => Some(slot + 2),
            _ => None,
        }
    }

    /// `Kind` 0: a memory cue.
    pub const MEMORY: u8 = 0;
}

/// A tree of named lists of tracks.
///
/// Playlists and histories are both this: rekordbox stores each as a tree
/// table plus a membership table, and nothing about reading one differs from
/// the other. `parent` indexes into this same structure, `NO_ID` for a root.
#[derive(Debug, Default, Clone)]
pub struct Playlists {
    pub ids: Vec<u64>,
    pub names: StrColumn,
    pub parent: Vec<u32>,
    pub seq: Vec<u32>,
    /// Row indices per playlist, in `TrackNo` order.
    pub members: Vec<Vec<Row>>,
}

impl Library {
    /// Sets the row count. Only the snapshot reader needs this: every other
    /// path counts rows as it pushes them.
    pub(crate) fn set_count(&mut self, count: usize) {
        self.count = count;
    }
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
    /// The row a track's display id names.
    pub fn row_of(&self, display_id: &str) -> Option<Row> {
        self.row_of_id(display_id.parse().ok()?)
    }

    /// The row a numeric track id names.
    ///
    /// The same map without the parse, for callers that already hold the
    /// number — a waveform request per row cannot afford to build a string to
    /// look one up.
    pub fn row_of_id(&self, id: u64) -> Option<Row> {
        self.row_by_id().get(&id).copied()
    }

    /// A track's cues, ordered by position.
    ///
    /// A copy, because the table is behind a lock and a track's cues are a
    /// handful of 16-byte values: copying them is cheaper than holding a
    /// guard across whatever the caller does next.
    pub fn cues_of(&self, row: Row) -> Vec<Cue> {
        self.cues.read().of(row).to_vec()
    }

    /// Reads the cue table. The guard is held only for the read.
    pub fn cues(&self) -> parking_lot::RwLockReadGuard<'_, Cues> {
        self.cues.read()
    }

    /// Swaps in one track's freshly-read cues, leaving every other track's
    /// and all the track columns alone.
    pub fn set_cues_of(&self, row: Row, cues: Vec<Cue>) {
        self.cues.write().replace(row, self.count, cues);
    }

    pub(crate) fn set_cues(&mut self, cues: Cues) {
        *self.cues.write() = cues;
    }

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

    /// The absolute path of a track's audio, by its display id.
    ///
    /// `folder_path` is already absolute in the reference library — it is the
    /// file's own location, not a share-relative one like the artwork.
    pub fn audio_path_of(&self, display_id: &str) -> Option<&str> {
        let wanted: u64 = display_id.parse().ok()?;
        let row = *self.row_by_id().get(&wanted)?;
        Some(self.folder_path.get(row as usize))
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

    /// The My Tag categories, for the filter bar's tag columns.
    pub fn my_tags(&self) -> &[TagCategory] {
        &self.my_tags
    }

    pub(crate) fn set_my_tags(&mut self, tags: Vec<TagCategory>) {
        self.my_tags = tags;
    }

    /// Reads the playlist tree. The guard is held only for the read.
    pub fn playlists(&self) -> parking_lot::RwLockReadGuard<'_, Playlists> {
        self.playlists.read()
    }

    /// Swaps in a freshly-read playlist tree, leaving the track columns alone.
    pub fn set_playlists(&self, playlists: Playlists) {
        *self.playlists.write() = playlists;
    }

    /// Reads the history tree: sessions, and the folders they are filed under.
    pub fn histories(&self) -> parking_lot::RwLockReadGuard<'_, Playlists> {
        self.histories.read()
    }

    pub fn set_histories(&self, histories: Playlists) {
        *self.histories.write() = histories;
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
        let cues = {
            let table = self.cues();
            table.cues.capacity() * std::mem::size_of::<Cue>() + table.index.capacity() * 4
        };
        let tags: usize = self
            .my_tags
            .iter()
            .map(|c| c.name.capacity() + c.tags.iter().map(String::capacity).sum::<usize>())
            .sum();
        let playlists = self.playlists().ids.capacity() * 8
            + self.playlists().names.heap_bytes()
            + self.playlists().members.iter().map(|m| m.capacity() * 4).sum::<usize>();
        vecs + strings + interners + ranks + cues + tags + playlists
    }
}
