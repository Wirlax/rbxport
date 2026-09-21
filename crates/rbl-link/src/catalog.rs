//! The database server's questions, answered out of the index.
//!
//! `rbl-dbserver` owns the protocol and asks a [`Catalog`] for rows; this is
//! the catalog over `rbl-index`, so a player browses the same columns the
//! browser does, sorted by the same ranks. The few fields the index does not
//! hold — bit rate, file size, the original artist — come from a point read
//! of the database through the [`Source`].
//!
//! Ids on the wire are ours to choose as long as a player can hand them
//! back: tracks go out as `djmdContent.ID`, which fits a `u32` in every
//! library measured; artists, albums, genres and labels as their interner
//! index plus one (0 means "none" in an album row); artwork as the track's
//! row plus two (1 means "no artwork" to a player). A CDJ-3000 asks for
//! artwork by the track's or album's id instead, and is answered that way.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rbl_anlz::Anlz;
use rbl_dbserver::catalog::{Analysis as Wanted, Catalog, Edit, Query, Row, Sort, TrackDetails, TrackScope};
use rbl_dbserver::item::TrackRow;
use rbl_dbserver::keys;
use rbl_index::{key::camelot_rank, Library, SortColumn, TrackSource, ViewSpec, NO_ID};

use crate::blobs::{self, Analysis, ExtendedCue};

/// Where the catalog reads from. The app implements this over its state so a
/// library reloaded behind a running link session is what the next request
/// sees, and so `rbl-link` needs nothing from the webview.
pub trait Source: Send + Sync {
    /// The library as it is now.
    fn library(&self) -> Option<Arc<Library>>;
    /// Where analysis files and artwork live.
    fn share_root(&self) -> PathBuf;
    /// The fields the index does not hold, by `djmdContent.ID`.
    fn details(&self, id: &str) -> Option<rbl_db::details::TrackDetails>;
    fn edit(&self, _edit: &Edit) -> bool { false }
}

/// Artwork larger than this is not sent: the protocol carries it whole in
/// one reply, and a player draws a thumbnail.
const MAX_ARTWORK: u64 = 4 * 1024 * 1024;

/// rekordbox keeps each artwork three ways beside `djmdContent.ImagePath`:
/// the file it names (`artwork.jpg`, up to a few hundred KB), a medium
/// `artwork_m.jpg` and a small `artwork_s.jpg`. It serves a player the medium
/// one (measured: rekordbox 7.2.11's 127 artwork replies to a CDJ-3000 ran
/// 2.5–32 KB, the `_m` files' range); sent the full file, a CDJ-3000 left
/// much of its list without art.
const MEDIUM_ARTWORK_SUFFIX: &str = "_m";

/// A player asks for a dozen blobs when it loads a track and two for every
/// row it draws; this many parsed analysis files stay in memory so each is
/// read once per track, not once per blob.
const ANALYSIS_CACHE: usize = 8;

/// The three analysis files of one track, parsed, or `None` where absent.
struct Parsed {
    row: rbl_index::Row,
    dat: Option<Anlz>,
    ext: Option<Anlz>,
    two_ex: Option<Anlz>,
}

/// The tracks players have loaded from us since the link started. The
/// beacon marks them from the players' status; the catalog greys their rows,
/// the way rekordbox greys what is in its link history. Cloned handles share
/// one set.
#[derive(Clone, Default)]
pub struct Played(Arc<Mutex<HashSet<u32>>>);

impl Played {
    pub fn mark(&self, track: u32) {
        self.0.lock().insert(track);
    }

    pub fn contains(&self, track: u32) -> bool {
        self.0.lock().contains(&track)
    }
}

/// The library as a player browses it.
pub struct IndexCatalog {
    source: Arc<dyn Source>,
    played: Played,
    /// Most recently used last.
    analysis: Mutex<Vec<Arc<Parsed>>>,
}

impl IndexCatalog {
    pub fn new(source: Arc<dyn Source>, played: Played) -> Self {
        Self { source, played, analysis: Mutex::new(Vec::with_capacity(ANALYSIS_CACHE)) }
    }

    fn row_of(library: &Library, id: u32) -> Option<rbl_index::Row> {
        library.row_of_id(u64::from(id))
    }

    /// The row a menu item's id names: the track with that id, or the first
    /// track with artwork on the album with that id. The two id spaces
    /// overlap for small numbers (an album's is its index plus one), and a
    /// track wins: it is what a player asks about far more often.
    fn item_row(library: &Library, id: u32) -> Option<rbl_index::Row> {
        if let Some(row) = Self::row_of(library, id) {
            return Some(row);
        }
        let album = id.checked_sub(1)?;
        library
            .album
            .iter()
            .enumerate()
            .find(|&(row, &of)| of == album && !library.artwork_path.get(row).is_empty())
            .and_then(|(row, _)| rbl_index::Row::try_from(row).ok())
    }

    /// The artwork file of a row, read whole: the medium file rekordbox
    /// keeps beside the one the library names, or the named one where
    /// rekordbox has not made it.
    fn artwork_at(&self, library: &Library, row: rbl_index::Row) -> Option<Vec<u8>> {
        let relative = library.artwork_path.get(row as usize);
        if relative.is_empty() {
            return None;
        }
        let share = self.source.share_root();
        let path = [medium_artwork(relative), relative.to_owned()]
            .iter()
            .filter_map(|candidate| resolve_under(&share, candidate))
            .find(|candidate| candidate.is_file())?;
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() > MAX_ARTWORK {
            return None;
        }
        std::fs::read(&path).ok()
    }

    fn track_id(library: &Library, row: rbl_index::Row) -> Option<u32> {
        library.ids.get(row as usize).and_then(|&id| u32::try_from(id).ok())
    }

    /// The Camelot key id a player uses, 1–24, or 0 for a key the wheel does
    /// not know.
    fn key_id(library: &Library, row: rbl_index::Row) -> u32 {
        match camelot_rank(library.key_name(row)) {
            u32::MAX => 0,
            rank => rank + 1,
        }
    }

    fn sort_column(sort: Sort, scope: &TrackScope) -> Option<SortColumn> {
        Some(match sort {
            // A playlist or history keeps its own order; every other list
            // is alphabetical, which is what rekordbox sent for TRACK.
            Sort::Default => match scope {
                TrackScope::Playlist(_) | TrackScope::History(_) | TrackScope::TagList => return None,
                _ => SortColumn::Title,
            },
            Sort::Alphabet => SortColumn::Title,
            Sort::Artist => SortColumn::Artist,
            Sort::Album => SortColumn::Album,
            Sort::Bpm => SortColumn::Bpm,
            Sort::Rating => SortColumn::Rating,
            Sort::Key => SortColumn::KeyCamelot,
        })
    }

    /// The rows of a scope, in the order the library holds them, each with
    /// the position a list gives it (0 where the list has none).
    fn scope_rows(library: &Library, scope: &TrackScope) -> Vec<(rbl_index::Row, u32)> {
        let all = || (0..u32::try_from(library.len()).unwrap_or(u32::MAX)).map(|row| (row, 0));
        match scope {
            TrackScope::All => all().collect(),
            TrackScope::TagList => library.tag_list().into_iter().enumerate().map(|(i, row)| (row, u32::try_from(i + 1).unwrap_or(u32::MAX))).collect(),
            TrackScope::Artist { artist, album } => {
                let artist = artist.wrapping_sub(1);
                let album = album.map(|a| a.wrapping_sub(1));
                all()
                    .filter(|&(row, _)| {
                        library.artist.get(row as usize) == Some(&artist)
                            && album.is_none_or(|a| library.album.get(row as usize) == Some(&a))
                    })
                    .collect()
            }
            TrackScope::Album(album) => {
                let album = album.wrapping_sub(1);
                all().filter(|&(row, _)| library.album.get(row as usize) == Some(&album)).collect()
            }
            TrackScope::Key { key, distance } => {
                let wanted: HashSet<u32> = keys::related(*key, *distance).into_iter().collect();
                all().filter(|&(row, _)| wanted.contains(&Self::key_id(library, row))).collect()
            }
            TrackScope::Playlist(id) => {
                let playlists = library.playlists();
                let Some(index) = playlists.index_of(u64::from(*id)) else {
                    return Vec::new();
                };
                playlists
                    .members
                    .get(index)
                    .map(|rows| {
                        rows.iter()
                            .enumerate()
                            .map(|(i, &row)| (row, u32::try_from(i + 1).unwrap_or(u32::MAX)))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            TrackScope::History(id) => {
                let histories = library.histories();
                let Some(index) = histories.index_of(u64::from(*id)) else {
                    return Vec::new();
                };
                histories
                    .members
                    .get(index)
                    .map(|rows| {
                        rows.iter()
                            .enumerate()
                            .map(|(i, &row)| (row, u32::try_from(i + 1).unwrap_or(u32::MAX)))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            TrackScope::DateAdded { year, month, day } => {
                let prefix = date_prefix(*year, *month, *day);
                all().filter(|&(row, _)| library.date_added.get(row as usize).starts_with(&prefix)).collect()
            }
            TrackScope::Search(text) => {
                let view = library.open_view(&ViewSpec {
                    source: TrackSource::Collection,
                    sort: SortColumn::Title,
                    descending: false,
                    query: text.clone(),
                    filter: rbl_index::TrackFilter::default(),
                });
                view.rows.iter().map(|&row| (row, 0)).collect()
            }
        }
    }

    fn tracks(library: &Library, scope: &TrackScope, sort: Sort) -> Vec<Row> {
        let mut rows = Self::scope_rows(library, scope);
        if let Some(column) = Self::sort_column(sort, scope) {
            let mut order: Vec<rbl_index::Row> = rows.iter().map(|&(row, _)| row).collect();
            library.sort_rows(&mut order, column, false);
            // Positions travel with their rows; the sort reorders the pairs.
            let mut position_of = std::collections::HashMap::with_capacity(rows.len());
            for (row, position) in rows {
                position_of.insert(row, position);
            }
            rows = order.into_iter().map(|row| (row, position_of.get(&row).copied().unwrap_or(0))).collect();
        }
        rows.into_iter()
            .filter_map(|(row, position)| Some(Row::Track { id: Self::track_id(library, row)?, position }))
            .collect()
    }

    /// The names of a lookup column that at least one track uses, sorted
    /// as the browser sorts them, as `(id, name)` rows.
    fn named(column: &[u32], interner: &rbl_index::strings::Interner) -> Vec<Row> {
        let mut used = vec![false; interner.len()];
        for &id in column {
            if let Some(slot) = used.get_mut(id as usize) {
                *slot = true;
            }
        }
        let mut ids: Vec<u32> = (0..u32::try_from(interner.len()).unwrap_or(u32::MAX))
            .filter(|&id| used.get(id as usize).copied().unwrap_or(false) && !interner.name(id).is_empty())
            .collect();
        ids.sort_by(|&a, &b| interner.folded(a).cmp(interner.folded(b)).then_with(|| a.cmp(&b)));
        ids.into_iter().map(|id| Row::Named { id: id + 1, name: interner.name(id).to_owned() }).collect()
    }

    fn artist_albums(library: &Library, artist: u32) -> Vec<Row> {
        let artist = artist.wrapping_sub(1);
        let mut albums: Vec<u32> = library
            .artist
            .iter()
            .zip(&library.album)
            .filter(|&(&a, &album)| a == artist && album != NO_ID)
            .map(|(_, &album)| album)
            .collect();
        albums.sort_unstable();
        albums.dedup();
        albums.sort_by(|&a, &b| library.albums.folded(a).cmp(library.albums.folded(b)));
        albums
            .into_iter()
            .filter(|&album| !library.albums.name(album).is_empty())
            .map(|album| Row::Named { id: album + 1, name: library.albums.name(album).to_owned() })
            .collect()
    }

    /// A folder's children — folders and lists alike — in `Seq` order.
    fn folder(lists: &rbl_index::Playlists, parent: u32) -> Vec<Row> {
        let parent_index = if parent == 0 {
            NO_ID
        } else {
            match lists.index_of(u64::from(parent)) {
                Some(index) => u32::try_from(index).unwrap_or(NO_ID),
                None => return Vec::new(),
            }
        };
        let mut children: Vec<usize> =
            (0..lists.len()).filter(|&i| lists.parent.get(i).copied() == Some(parent_index)).collect();
        children.sort_by_key(|&i| lists.seq.get(i).copied().unwrap_or(0));
        children
            .into_iter()
            .filter_map(|i| {
                Some(Row::List {
                    id: u32::try_from(*lists.ids.get(i)?).ok()?,
                    name: lists.name(i).to_owned(),
                    folder: lists.is_folder(i),
                    position: lists.seq.get(i).copied().unwrap_or(0),
                })
            })
            .collect()
    }

    /// Every history session, newest first. `[ASSUME]` the order: the one
    /// capture held a single session.
    fn histories(lists: &rbl_index::Playlists) -> Vec<Row> {
        let mut sessions: Vec<Row> = (0..lists.len())
            .filter(|&i| !lists.is_folder(i))
            .filter_map(|i| {
                Some(Row::Named { id: u32::try_from(*lists.ids.get(i)?).ok()?, name: lists.name(i).to_owned() })
            })
            .collect();
        sessions.reverse();
        sessions
    }

    /// The distinct values of one part of the date-added column under a
    /// prefix: years (newest first), or months and days (ascending).
    fn date_parts(library: &Library, prefix: &str, at: std::ops::Range<usize>, newest_first: bool) -> Vec<Row> {
        let mut values: Vec<u32> = (0..library.len())
            .filter_map(|row| {
                let date = library.date_added.get(row);
                if !date.starts_with(prefix) {
                    return None;
                }
                date.get(at.clone())?.parse().ok()
            })
            .collect();
        values.sort_unstable();
        values.dedup();
        if newest_first {
            values.reverse();
        }
        values.into_iter().filter(|&v| v != 0).map(Row::Date).collect()
    }

    /// The parsed analysis files of a track, from the cache or the disk.
    fn parsed(&self, library: &Library, row: rbl_index::Row) -> Option<Arc<Parsed>> {
        {
            let mut cache = self.analysis.lock();
            if let Some(at) = cache.iter().position(|p| p.row == row) {
                let hit = cache.remove(at);
                cache.push(Arc::clone(&hit));
                return Some(hit);
            }
        }
        let relative = library.analysis_path.get(row as usize);
        if relative.is_empty() {
            return None;
        }
        let dat_path = rbl_anlz::resolve(&self.source.share_root(), relative);
        let read = |path: &std::path::Path| std::fs::read(path).ok().and_then(|bytes| rbl_anlz::parse(&bytes).ok());
        let parsed = Arc::new(Parsed {
            row,
            dat: read(&dat_path),
            ext: read(&rbl_anlz::sibling(&dat_path, "EXT")),
            two_ex: read(&rbl_anlz::sibling(&dat_path, "2EX")),
        });
        let mut cache = self.analysis.lock();
        if cache.len() >= ANALYSIS_CACHE {
            cache.remove(0);
        }
        cache.push(Arc::clone(&parsed));
        Some(parsed)
    }

    /// Forgets parsed analysis files, for after a library reload or an
    /// analysis run: the next request reads the files again.
    pub fn forget_analysis(&self) {
        self.analysis.lock().clear();
    }
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` as a prefix of `StockDate`.
fn date_prefix(year: u32, month: Option<u32>, day: Option<u32>) -> String {
    match (month, day) {
        (Some(m), Some(d)) => format!("{year:04}-{m:02}-{d:02}"),
        (Some(m), None) => format!("{year:04}-{m:02}"),
        _ => format!("{year:04}"),
    }
}

impl Catalog for IndexCatalog {
    fn list(&self, query: &Query) -> Vec<Row> {
        let Some(library) = self.source.library() else {
            return Vec::new();
        };
        match query {
            Query::Artists(_) => Self::named(&library.artist, &library.artists),
            Query::Albums(_) => Self::named(&library.album, &library.albums),
            Query::ArtistAlbums(artist) => Self::artist_albums(&library, *artist),
            Query::Folder(parent) => Self::folder(&library.playlists(), *parent),
            Query::Histories => Self::histories(&library.histories()),
            Query::Years => Self::date_parts(&library, "", 0..4, true),
            Query::Months(year) => Self::date_parts(&library, &date_prefix(*year, None, None), 5..7, false),
            Query::Days { year, month } => {
                Self::date_parts(&library, &date_prefix(*year, Some(*month), None), 8..10, false)
            }
            Query::Tracks { scope, sort } => Self::tracks(&library, scope, *sort),
        }
    }

    fn track_row(&self, id: u32) -> Option<TrackRow> {
        let library = self.source.library()?;
        let row = Self::row_of(&library, id)?;
        let at = row as usize;
        Some(TrackRow {
            id,
            title: library.title.get(at).to_owned(),
            comment: library.comment.get(at).to_owned(),
            key: Self::key_id(&library, row),
            key_name: library.key_name(row).to_owned(),
            artwork: if library.artwork_path.get(at).is_empty() { 0 } else { row.saturating_add(2) },
            bpm_x100: library.bpm_x100.get(at).copied().unwrap_or(0),
        })
    }

    fn track(&self, id: u32) -> Option<TrackDetails> {
        let library = self.source.library()?;
        let row = Self::row_of(&library, id)?;
        let at = row as usize;
        let lookup_id = |ids: &[u32]| ids.get(at).map_or(0, |&v| if v == NO_ID { 0 } else { v + 1 });
        let path = library.folder_path.get(at).to_owned();
        // The database's row for what the index leaves out; a read that
        // fails costs those fields, not the reply.
        let details = self.source.details(&id.to_string());
        let file_size = details
            .as_ref()
            .map(|d| d.file_size)
            .filter(|&size| size > 0)
            .or_else(|| std::fs::metadata(&path).ok().map(|m| m.len()))
            .unwrap_or(0);
        Some(TrackDetails {
            row: self.track_row(id)?,
            artist_id: lookup_id(&library.artist),
            artist: library.artist_name(row).to_owned(),
            album_id: lookup_id(&library.album),
            album: library.album_name(row).to_owned(),
            duration_s: library.length_sec.get(at).copied().unwrap_or(0),
            rating: u32::from(library.rating.get(at).copied().unwrap_or(0)),
            colour: u32::from(library.color.get(at).copied().unwrap_or(0)),
            genre_id: lookup_id(&library.genre),
            genre: library.genre_name(row).to_owned(),
            date_added: library.date_added.get(at).to_owned(),
            year: details.as_ref().map_or(0, |d| d.year),
            bit_rate_kbps: details.as_ref().map_or(0, |d| d.bitrate),
            label_id: lookup_id(&library.label),
            label: library.label_name(row).to_owned(),
            original_artist: details.as_ref().map(|d| d.original_artist.clone()).unwrap_or_default(),
            remixer: details.as_ref().map(|d| d.remixer.clone()).unwrap_or_default(),
            path,
            file_size: u32::try_from(file_size).unwrap_or(u32::MAX),
            file_type: details.as_ref().map_or(0, |d| d.file_type),
        })
    }

    fn artwork(&self, id: u32) -> Option<Vec<u8>> {
        let library = self.source.library()?;
        let row = id.checked_sub(2)?;
        self.artwork_at(&library, row)
    }

    fn item_artwork(&self, id: u32) -> Option<Vec<u8>> {
        let library = self.source.library()?;
        let row = Self::item_row(&library, id)?;
        self.artwork_at(&library, row)
    }

    fn analysis(&self, track: u32, what: &Wanted) -> Option<Vec<u8>> {
        let library = self.source.library()?;
        let row = Self::row_of(&library, track)?;
        match what {
            // rekordbox's plain cue-list reply (2504) is a fixed 1,604-byte
            // buffer, all zero for a track with no old-format cues. A player
            // reads its real cues from the extended list (2b04); the plain
            // reply must still arrive with these bytes and a success status,
            // or a CDJ-3000 hangs mid-load waiting for it (`blobs`).
            Wanted::CueList => Some(blobs::cue_list_blob()),
            Wanted::ExtendedCueList => {
                let cues: Vec<ExtendedCue> = library.cues_of(row).iter().map(ExtendedCue::from).collect();
                Some(blobs::extended_cues_blob(&cues).0)
            }
            _ => {
                let parsed = self.parsed(&library, row)?;
                let analysis =
                    Analysis { dat: parsed.dat.as_ref(), ext: parsed.ext.as_ref(), two_ex: parsed.two_ex.as_ref() };
                match what {
                    Wanted::BeatGrid => analysis.beat_grid(),
                    Wanted::WaveformPreview => analysis.waveform_preview(),
                    Wanted::WaveformDetail => analysis.waveform_detail(),
                    Wanted::Tag { fourcc, extension } => analysis.tag(fourcc, extension),
                    Wanted::CueList | Wanted::ExtendedCueList => None,
                }
            }
        }
    }

    fn grid_offset(&self, track: u32) -> i16 {
        let offset = || {
            let library = self.source.library()?;
            let parsed = self.parsed(&library, Self::row_of(&library, track)?)?;
            parsed.dat.as_ref()?.grid_offset()
        };
        offset().unwrap_or(0)
    }

    fn edit(&self, edit: &Edit) -> bool {
        let success = self.source.edit(edit);
        if success && matches!(edit, Edit::GridOffset { .. }) { self.forget_analysis(); }
        success
    }

    fn tagged(&self, track: u32) -> bool {
        self.source.library().is_some_and(|lib| Self::row_of(&lib, track).is_some_and(|row| lib.tag_list().contains(&row)))
    }

    fn filter_rows(&self, rows: &mut Vec<Row>, filter: &rbl_dbserver::filter::TrackFilter) {
        if !filter.enabled { return; }
        let Some(lib) = self.source.library() else { rows.clear(); return; };
        rows.retain(|row| match row {
            Row::Track { id, .. } => Self::row_of(&lib, *id).is_some_and(|row| {
                let at = row as usize;
                filter.matches(lib.bpm_x100[at], Self::key_id(&lib, row), u32::from(lib.rating[at]), u32::from(lib.color[at]))
            }),
            _ => true,
        });
    }

    fn played(&self, track: u32) -> bool {
        self.played.contains(track)
    }
}

/// The medium file beside the artwork `ImagePath` names: `_m` before the
/// extension. A path with no extension is returned as it is.
fn medium_artwork(relative: &str) -> String {
    let stem_end = relative.rfind('.').filter(|&dot| !relative[dot..].contains(['/', '\\']));
    match stem_end {
        Some(dot) => format!("{}{MEDIUM_ARTWORK_SUFFIX}{}", &relative[..dot], &relative[dot..]),
        None => relative.to_owned(),
    }
}

/// `share_root/relative`, refused if the relative path climbs out.
fn resolve_under(share: &std::path::Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim_start_matches(['/', '\\']);
    if relative.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    Some(share.join(relative))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use rbl_index::testing::{add_folder, add_history, add_playlist, library_from, TestTrack};

    struct Fixed(Arc<Library>);

    impl Source for Fixed {
        fn library(&self) -> Option<Arc<Library>> {
            Some(Arc::clone(&self.0))
        }
        fn share_root(&self) -> PathBuf {
            PathBuf::from("/nonexistent")
        }
        fn details(&self, _id: &str) -> Option<rbl_db::details::TrackDetails> {
            None
        }
    }

    fn library() -> Library {
        let t = |id, title, artist, album, key, bpm, date| TestTrack {
            id,
            title,
            artist,
            album,
            key,
            bpm_x100: bpm,
            date_added: date,
            path: "/Volumes/SD/RB/x.mp3",
            ..TestTrack::default()
        };
        let mut lib = library_from(&[
            t(10, "Zebra", "Bob", "Second", "Am", 12_800, "2025-03-04"),
            t(11, "Apple", "Alice", "First", "Abm", 12_000, "2026-01-09"),
            t(12, "Mango", "Carol", "Second", "B", 13_000, "2026-01-20"),
        ]);
        let folder = add_folder(&mut lib, "Crates");
        add_playlist(&mut lib, "Warm up", &[2, 0]);
        let _ = folder;
        add_history(&mut lib, "HISTORY 2026-09-01", &[1]);
        add_history(&mut lib, "HISTORY 2026-09-02", &[0]);
        lib
    }

    fn catalog() -> IndexCatalog {
        IndexCatalog::new(Arc::new(Fixed(Arc::new(library()))), Played::default())
    }

    fn ids(rows: &[Row]) -> Vec<u32> {
        rows.iter()
            .map(|r| match r {
                Row::Track { id, .. } | Row::Named { id, .. } | Row::List { id, .. } => *id,
                Row::Date(v) => *v,
            })
            .collect()
    }

    #[test]
    fn the_track_menu_is_alphabetical_by_default_and_sorts_on_request() {
        let c = catalog();
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::All, sort: Sort::Default })), [11, 12, 10]);
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::All, sort: Sort::Bpm })), [11, 10, 12]);
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::All, sort: Sort::Artist })), [11, 10, 12]);
    }

    #[test]
    fn key_sort_uses_the_wheel_and_preserves_playlist_positions() {
        let c = catalog();
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::All, sort: Sort::Key })), [11, 12, 10]);
        let mut lib = library();
        let playlist = add_playlist(&mut lib, "Keys", &[0, 2, 1]);
        let playlist = u32::try_from(lib.playlists().ids[playlist]).unwrap();
        let c = IndexCatalog::new(Arc::new(Fixed(Arc::new(lib))), Played::default());
        assert_eq!(c.list(&Query::Tracks { scope: TrackScope::Playlist(playlist), sort: Sort::Key }), vec![
            Row::Track { id: 11, position: 3 },
            Row::Track { id: 12, position: 2 },
            Row::Track { id: 10, position: 1 },
        ]);
    }

    #[test]
    fn artists_and_their_albums_carry_interner_ids_plus_one() {
        // The test builder interns one row per track, as `djmdArtist` can:
        // a name under two ids is two menu rows, which is what rekordbox
        // sends too ("Aaliyah" and "Aaliyah ft. Dash!e" were separate ids).
        let c = catalog();
        let artists = c.list(&Query::Artists(Sort::Default));
        assert_eq!(artists, vec![
            Row::Named { id: 2, name: "Alice".into() },
            Row::Named { id: 1, name: "Bob".into() },
            Row::Named { id: 3, name: "Carol".into() },
        ]);
        assert_eq!(c.list(&Query::ArtistAlbums(2)), vec![Row::Named { id: 2, name: "First".into() }]);
        assert_eq!(
            ids(&c.list(&Query::Tracks { scope: TrackScope::Artist { artist: 2, album: Some(2) }, sort: Sort::Default })),
            [11]
        );
        assert_eq!(
            ids(&c.list(&Query::Tracks { scope: TrackScope::Artist { artist: 2, album: None }, sort: Sort::Default })),
            [11]
        );
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::Album(1), sort: Sort::Default })), [10]);
    }

    #[test]
    fn keys_are_camelot_ids_and_related_keys_widen_the_list() {
        let c = catalog();
        // Abm is 1A = id 1; B is 1B = id 2; Am is 8A = id 15.
        assert_eq!(c.track_row(11).unwrap().key, 1);
        assert_eq!(c.track_row(12).unwrap().key, 2);
        assert_eq!(c.track_row(10).unwrap().key, 15);
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::Key { key: 1, distance: 0 }, sort: Sort::Default })), [11]);
        assert_eq!(
            ids(&c.list(&Query::Tracks { scope: TrackScope::Key { key: 1, distance: 1 }, sort: Sort::Default })),
            [11, 12]
        );
    }

    #[test]
    fn playlists_keep_their_own_order_and_number_their_rows() {
        let c = catalog();
        let root = c.list(&Query::Folder(0));
        assert_eq!(root.len(), 2);
        let Row::List { id: warm_up, folder: false, .. } = root[1] else { panic!("{root:?}") };
        let rows = c.list(&Query::Tracks { scope: TrackScope::Playlist(warm_up), sort: Sort::Default });
        assert_eq!(rows, vec![Row::Track { id: 12, position: 1 }, Row::Track { id: 10, position: 2 }]);
        // Sorted on request, positions still their own.
        let rows = c.list(&Query::Tracks { scope: TrackScope::Playlist(warm_up), sort: Sort::Alphabet });
        assert_eq!(rows, vec![Row::Track { id: 12, position: 1 }, Row::Track { id: 10, position: 2 }]);
    }

    #[test]
    fn histories_come_newest_first() {
        let c = catalog();
        let names: Vec<String> = c
            .list(&Query::Histories)
            .into_iter()
            .map(|r| match r {
                Row::Named { name, .. } => name,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["HISTORY 2026-09-02", "HISTORY 2026-09-01"]);
    }

    #[test]
    fn date_added_walks_years_months_and_days() {
        let c = catalog();
        assert_eq!(c.list(&Query::Years), vec![Row::Date(2026), Row::Date(2025)]);
        assert_eq!(c.list(&Query::Months(2026)), vec![Row::Date(1)]);
        assert_eq!(c.list(&Query::Days { year: 2026, month: 1 }), vec![Row::Date(9), Row::Date(20)]);
        let january = TrackScope::DateAdded { year: 2026, month: Some(1), day: None };
        assert_eq!(ids(&c.list(&Query::Tracks { scope: january, sort: Sort::Default })), [11, 12]);
        let the_ninth = TrackScope::DateAdded { year: 2026, month: Some(1), day: Some(9) };
        assert_eq!(ids(&c.list(&Query::Tracks { scope: the_ninth, sort: Sort::Default })), [11]);
    }

    #[test]
    fn search_matches_the_browser_search() {
        let c = catalog();
        assert_eq!(ids(&c.list(&Query::Tracks { scope: TrackScope::Search("second".into()), sort: Sort::Default })), [12, 10]);
        assert!(c.list(&Query::Tracks { scope: TrackScope::Search("nothing".into()), sort: Sort::Default }).is_empty());
    }

    #[test]
    fn track_details_come_from_the_index_with_the_absolute_path() {
        let c = catalog();
        let details = c.track(10).unwrap();
        assert_eq!(details.artist, "Bob");
        assert_eq!(details.album, "Second");
        assert_eq!(details.path, "/Volumes/SD/RB/x.mp3");
        assert_eq!(details.date_added, "2025-03-04");
        assert_eq!(details.row.bpm_x100, 12_800);
        assert!(c.track(99).is_none());
    }

    #[test]
    fn a_track_without_analysis_has_no_blobs() {
        let c = catalog();
        assert!(c.analysis(10, &Wanted::BeatGrid).is_none());
        // The plain cue list (2504) is a fixed 1,604-byte buffer rekordbox
        // sends for every track, zero here because there are no old-format
        // cues; the extended list (2b04) carries the real ones.
        assert_eq!(c.analysis(10, &Wanted::CueList).unwrap(), vec![0_u8; 1604]);
        assert_eq!(c.analysis(10, &Wanted::ExtendedCueList).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn a_cdj_3000_names_the_track_or_album_whose_artwork_it_wants() {
        let mut lib = library();
        for relative in ["", "/PIONEER/Artwork/a/artwork.jpg", "/PIONEER/Artwork/b/artwork.jpg"] {
            lib.artwork_path.push(relative);
        }
        // Tracks by id, then albums: the test interner gives each track its
        // own album entry, so album ids 1, 2, 3 hold tracks 10, 11, 12.
        assert_eq!(IndexCatalog::item_row(&lib, 12), Some(2));
        assert_eq!(IndexCatalog::item_row(&lib, 11), Some(1));
        assert_eq!(IndexCatalog::item_row(&lib, 3), Some(2), "album 3's track has artwork");
        assert_eq!(IndexCatalog::item_row(&lib, 2), Some(1));
        assert_eq!(IndexCatalog::item_row(&lib, 1), None, "album 1's only track has none");
        assert_eq!(IndexCatalog::item_row(&lib, 99), None);
    }

    #[test]
    fn artwork_is_the_medium_file_beside_the_one_the_library_names() {
        assert_eq!(medium_artwork("/PIONEER/Artwork/5ba/0a225-f6b2/artwork.jpg"), "/PIONEER/Artwork/5ba/0a225-f6b2/artwork_m.jpg");
        assert_eq!(medium_artwork("art.v2.png"), "art.v2_m.png");
        assert_eq!(medium_artwork("/a.b/artwork"), "/a.b/artwork");
    }
}
