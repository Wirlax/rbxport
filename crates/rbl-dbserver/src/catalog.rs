//! What a database server needs from the library to answer a player.
//!
//! The server owns the protocol — request kinds, item layouts, paging — and
//! asks the catalog for rows. The catalog is implemented over the index in
//! `rbl-link`; the tests here use a small in-memory one.

use crate::item::TrackRow;

/// How a track list is ordered: the ids of the sort menu (`1400`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sort {
    Default,
    Alphabet,
    Artist,
    Album,
    Bpm,
    Rating,
    Key,
}

impl Sort {
    pub fn from_id(id: u32) -> Self {
        match id {
            1 => Self::Alphabet,
            2 => Self::Artist,
            3 => Self::Album,
            4 => Self::Bpm,
            5 => Self::Rating,
            0xc => Self::Key,
            _ => Self::Default,
        }
    }
}

/// Which tracks a list holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TrackScope {
    All,
    /// An artist's tracks, on one album or (`None`) all of them.
    Artist { artist: u32, album: Option<u32> },
    Album(u32),
    /// Tracks in a key, widened by the related-key distance (0–2).
    Key { key: u32, distance: u32 },
    Playlist(u32),
    History(u32),
    /// Tracks added in a year, a month of it, or a day of that month.
    DateAdded { year: u32, month: Option<u32>, day: Option<u32> },
    /// A player's text search: the string as typed (upper case).
    Search(String),
}

/// A menu whose rows come from the library.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Query {
    Artists(Sort),
    Albums(Sort),
    /// An artist's albums; the server puts `⟨ALL⟩` before them.
    ArtistAlbums(u32),
    /// A playlist folder's children; 0 is the root.
    Folder(u32),
    Histories,
    Years,
    Months(u32),
    Days { year: u32, month: u32 },
    Tracks { scope: TrackScope, sort: Sort },
}

/// One row of a library menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// An artist, album, genre, key or session: id and name.
    Named { id: u32, name: String },
    /// A playlist folder or list, with its position under its parent.
    List { id: u32, name: String, folder: bool, position: u32 },
    /// A year, month or day.
    Date(u32),
    /// A track by id, and its position where the list has one (a playlist's
    /// order, an album's track number), else 0. The row itself is fetched
    /// when it is rendered, so a 40,000-track list is a vector of ids.
    Track { id: u32, position: u32 },
}

/// Everything the metadata and track-info replies show about one track.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackDetails {
    pub row: TrackRow,
    pub artist_id: u32,
    pub artist: String,
    pub album_id: u32,
    pub album: String,
    pub duration_s: u32,
    pub rating: u32,
    pub colour: u32,
    pub genre_id: u32,
    pub genre: String,
    /// `YYYY-MM-DD`.
    pub date_added: String,
    pub year: u32,
    pub bit_rate_kbps: u32,
    pub label_id: u32,
    pub label: String,
    pub original_artist: String,
    pub remixer: String,
    /// The absolute host path a player opens over NFS.
    pub path: String,
    pub file_size: u32,
}

/// The per-track blobs a player asks for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Analysis {
    /// A tag copied whole from the `.EXT` or `.2EX` analysis file, named by
    /// its fourcc (`b"PWV4"`) and the file's extension (`b"EXT"`).
    Tag { fourcc: [u8; 4], extension: [u8; 3] },
    BeatGrid,
    CueList,
    ExtendedCueList,
    WaveformPreview,
    WaveformDetail,
}

/// The library, as a player browses it.
pub trait Catalog: Send + Sync {
    /// The rows of a menu, whole and in order.
    fn list(&self, query: &Query) -> Vec<Row>;

    /// A track row by id, for the rows a list names by id.
    fn track_row(&self, id: u32) -> Option<TrackRow>;

    /// The whole record, for metadata and track info.
    fn track(&self, id: u32) -> Option<TrackDetails>;

    /// Artwork bytes (JPEG) by `djmdContent.ArtworkID`.
    fn artwork(&self, id: u32) -> Option<Vec<u8>>;

    /// A track's analysis blob, in the layout the reply carries.
    fn analysis(&self, track: u32, what: &Analysis) -> Option<Vec<u8>>;

    /// A player loaded (`Some`) or unloaded (`None`) one of our tracks.
    fn loaded(&self, _player: u8, _track: Option<u32>) {}
}
