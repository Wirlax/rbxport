//! The information panel's commands: one track's full record, the lists its
//! dropdowns offer, and the fields it may write.
//!
//! The record is a point read from `djmdContent` by id, not a widening of the
//! columnar index — see `rbl_db::details`. The deck's INFO tab reads the same
//! record, which is why the DTO carries everything a panel could show rather
//! than only what the row DTO lacks.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::commands::{blocking, edit, write_error, Touched};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::AppState;

/// One track, in full. About 1 KB of JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDetailsDto {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub original_artist: String,
    pub composer: String,
    pub remixer: String,
    pub lyricist: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    pub comment: String,
    pub mix_name: String,
    pub message: String,
    /// `"0"` or empty for none, `"1"` to `"8"` for rekordbox's eight colours.
    pub color: String,
    pub rating: u8,
    pub bpm_x100: u32,
    pub duration_sec: u32,
    pub year: u32,
    pub track_number: u32,
    pub disc_number: u32,
    pub play_count: u32,
    /// rekordbox's own code: 1 MP3, 4 M4A, 5 FLAC, 11 WAV, 12 AIFF.
    pub file_type: u32,
    pub file_size: u64,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub date_created: String,
    pub release_date: String,
    pub path: String,
    pub hot_cue_auto_load: bool,
    pub publish: bool,
    /// Whether `rbl://artwork/<id>` will serve anything for this track.
    pub has_artwork: bool,
}

/// What the Info tab's dropdowns offer.
///
/// Keys and genres come from the index's interners — what the library holds
/// — rather than a fixed list, because rekordbox's own dropdowns offer what
/// the library holds. 25 keys and 448 genres in the reference library, well
/// inside the response cap.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackLookupsDto {
    pub keys: Vec<String>,
    pub genres: Vec<String>,
}

/// Names past this many are dropped, so a library with an absurd genre list
/// cannot push one response over the cap.
const MAX_NAMES: usize = 2000;

#[tauri::command]
pub async fn track_details(
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<TrackDetailsDto> {
    let state = Arc::clone(&state);
    blocking("track_details", move || {
        let library = state.library()?;
        let has_artwork = library.artwork_path_of(&track).is_some_and(|p| !p.is_empty());
        let details = state
            .read_db(|db| rbl_db::details::track_details(db.connection(), &track))
            .map_err(write_error)?;
        let Some(d) = details else {
            return Err(AppError::new(ErrorKind::NotFound, "That track is no longer in the library.")
                .with_detail(format!("track {track}")));
        };
        Ok(TrackDetailsDto {
            id: d.id,
            title: d.title,
            artist: d.artist,
            album: d.album,
            album_artist: d.album_artist,
            original_artist: d.original_artist,
            composer: d.composer,
            remixer: d.remixer,
            lyricist: d.lyricist,
            genre: d.genre,
            label: d.label,
            key: d.key,
            comment: d.comment,
            mix_name: d.mix_name,
            message: d.message,
            color: d.color,
            rating: d.rating,
            bpm_x100: d.bpm_x100,
            duration_sec: d.duration_sec,
            year: d.year,
            track_number: d.track_number,
            disc_number: d.disc_number,
            play_count: d.play_count,
            file_type: d.file_type,
            file_size: d.file_size,
            bitrate: d.bitrate,
            sample_rate: d.sample_rate,
            bit_depth: d.bit_depth,
            date_created: d.date_created,
            release_date: d.release_date,
            path: d.path,
            hot_cue_auto_load: d.hot_cue_auto_load,
            publish: d.publish,
            has_artwork,
        })
    })
    .await
}

#[tauri::command]
pub async fn track_lookups(state: State<'_, Arc<AppState>>) -> AppResult<TrackLookupsDto> {
    let library = state.library()?;
    blocking("track_lookups", move || {
        let names = |interner: &rbl_index::strings::Interner| -> Vec<String> {
            let mut out: Vec<String> = (0..interner.len())
                .filter_map(|i| u32::try_from(i).ok())
                .map(|i| interner.name(i))
                .filter(|n| !n.is_empty())
                .take(MAX_NAMES)
                .map(str::to_owned)
                .collect();
            out.sort_unstable_by_key(|n| n.to_lowercase());
            out
        };
        Ok(TrackLookupsDto { keys: names(&library.keys), genres: names(&library.genres) })
    })
    .await
}

/// Add Artwork: the image at `image` is filed in the share tree and the
/// track points at it.
#[tauri::command]
pub async fn add_artwork<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
    image: String,
) -> AppResult<u32> {
    edit(app, state, "add_artwork", Touched::Tracks, move |w| {
        w.set_artwork(&track, Some(std::path::Path::new(&image))).map(|_| ())
    })
    .await
}

/// Delete Artwork: the track points at no image; the file stays.
#[tauri::command]
pub async fn clear_artwork<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
) -> AppResult<u32> {
    edit(app, state, "clear_artwork", Touched::Tracks, move |w| w.set_artwork(&track, None).map(|_| ())).await
}

/// Writes one of the Info tab's editable fields.
///
/// `field` is the wire name — `title`, `artist`, `year`, … — and the set of
/// names the writer accepts is the whole list of what is safe to write; a
/// name it does not know is refused here rather than mapped to a guess.
#[tauri::command]
pub async fn set_track_field<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    track: String,
    field: String,
    value: String,
) -> AppResult<u32> {
    let Some(which) = rbl_db::write::TrackField::parse(&field) else {
        return Err(AppError::new(ErrorKind::ReadOnly, format!("{field} cannot be edited here.")));
    };
    edit(app, state, "set_track_field", Touched::Tracks, move |w| {
        w.set_field(&track, which, &value).map(|_| ())
    })
    .await
}
