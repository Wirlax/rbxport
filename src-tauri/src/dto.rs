//! Wire types.
//!
//! Field names are camelCase to match `src/ipc/types.ts`. Rows are kept flat
//! and small — a page of 64 is roughly 15 KB of JSON, well inside the 64 KB
//! response cap.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDto {
    pub id: String,
    pub track_no: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub comment: String,
    pub bpm_x100: u32,
    pub key: String,
    pub duration_sec: u32,
    pub rating: u8,
    pub analysed: u8,
    pub date_added: String,
    pub release_date: String,
    /// Hot-cue letters present on the track. Populated once `rbl-anlz` lands.
    pub cues: String,
    /// Deterministic tint, drawn when a track has no artwork — a little under
    /// half the reference library.
    pub artwork_hue: u16,
    /// Whether `rbl://artwork/<id>` will serve anything for this track.
    pub has_artwork: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeDto {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummaryDto {
    pub track_count: u32,
    pub playlist_count: u32,
    pub read_only: bool,
    pub db_version: Option<i64>,
    /// Milliseconds the library took to open and index; shown in diagnostics.
    pub load_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewHandleDto {
    pub view_id: u32,
    pub len: u32,
    pub gen: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TrackSourceDto {
    #[serde(rename = "collection")]
    Collection,
    #[serde(rename = "playlist")]
    Playlist { id: String },
    #[serde(rename = "history")]
    // The id is accepted for forward compatibility; histories are not indexed yet.
    History {
        #[allow(dead_code, reason = "accepted from the wire, used once histories are indexed")]
        id: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewSpecDto {
    pub source: TrackSourceDto,
    pub sort: String,
    pub descending: bool,
    pub query: String,
}

/// A track whose audio file is no longer where the library says.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingTrackDto {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Where the library still expects it.
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingTracksDto {
    /// Every missing track, not just the ones listed.
    pub total: u32,
    pub tracks: Vec<MissingTrackDto>,
}

/// One cue point, as the interface needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueDto {
    pub position_ms: u32,
    /// `A` to `P` for a hot cue, empty for a memory cue.
    pub letter: String,
    pub memory: bool,
}

/// What an import batch did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportDto {
    pub imported: u32,
    /// One line per file that was not imported, saying why.
    pub skipped: Vec<String>,
}

/// One beat of the grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatDto {
    pub time_ms: u32,
    /// The first beat of a bar, drawn heavier than the rest.
    pub downbeat: bool,
}

/// What an export wrote.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportDto {
    pub tracks: u32,
    pub playlists: u32,
    pub bytes_copied: u64,
    pub analysis_files: u32,
    /// Tracks already on the stick, unchanged, that did not need copying again.
    pub reused: u32,
    /// Tracks taken off the stick because the playlist no longer holds them.
    pub removed: u32,
    /// Tracks left out because their audio was missing or unreadable.
    pub skipped: Vec<String>,
    /// Whether the export read back correctly with the independent parser.
    pub verified: bool,
}
