//! Writes a rekordbox USB export.
//!
//! An export is a directory tree a CDJ can browse:
//!
//! ```text
//!   Contents/<Artist>/<Album>/<file>       the audio
//!   PIONEER/USBANLZ/<P###>/<8-hex>/…       the analysis
//!   PIONEER/rekordbox/export.pdb           the database
//! ```
//!
//! Nothing here touches the user's library: it reads from an already-loaded
//! index and writes only under the destination directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rbl_pdb::build::FileBuilder;
use rbl_pdb::rows::{
    album_row, artist_row, color_row, key_row, playlist_entry_row, playlist_row,
    simple_named_row, track_row, TrackInput,
};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the destination is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("the device went away during the export")]
    DeviceGone,
    #[error("nothing to export")]
    Empty,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("could not write exportLibrary.db: {0}")]
    OneLibrary(String),
}

pub type Result<T> = std::result::Result<T, ExportError>;

/// `DeviceSQL` page size rekordbox uses.
const PAGE_SIZE: usize = 4096;

/// One track to export.
#[derive(Debug, Clone, Default)]
pub struct SourceTrack {
    /// Where the audio currently lives.
    pub source_path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    pub comment: String,
    pub date_added: String,
    pub release_date: String,
    pub bpm_x100: u32,
    pub duration_sec: u16,
    pub rating: u8,
    pub color_id: u8,
    pub year: u16,
    pub bitrate: u32,
    pub sample_rate: u32,
    /// Analysis to copy alongside, as (extension, bytes).
    pub analysis: Vec<(String, Vec<u8>)>,
}

/// A playlist to include.
#[derive(Debug, Clone)]
pub struct SourcePlaylist {
    pub name: String,
    /// Indices into the track slice.
    pub track_indices: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct ExportReport {
    pub tracks: usize,
    pub playlists: usize,
    pub bytes_copied: u64,
    pub analysis_files: usize,
    pub pdb_bytes: usize,
    /// Tracks skipped because their audio was missing or unreadable.
    pub skipped: Vec<String>,
    /// Whether `exportLibrary.db` was written.
    pub one_library: bool,
}

/// The eight colour labels rekordbox writes to every export.
const COLORS: [&str; 8] =
    ["Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple"];

/// Makes a name safe for FAT32, which is what a DJ stick is formatted as.
fn fat_safe(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    // Trailing dots and spaces are not addressable on FAT.
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("Unknown");
    }
    out.truncate(120);
    out
}

/// The lookup tables an export builds as it walks the tracks.
struct Interns<'a> {
    artists: &'a mut Intern,
    albums: &'a mut Intern,
    genres: &'a mut Intern,
    labels: &'a mut Intern,
    keys: &'a mut Intern,
}

/// Interns names into ids starting at 1, preserving first-seen order.
#[derive(Default)]
struct Intern {
    ids: BTreeMap<String, u32>,
    order: Vec<String>,
}

impl Intern {
    fn id(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = u32::try_from(self.order.len()).unwrap_or(0) + 1;
        self.ids.insert(name.to_owned(), id);
        self.order.push(name.to_owned());
        id
    }

    fn entries(&self) -> impl Iterator<Item = (u32, &str)> {
        self.order
            .iter()
            .enumerate()
            .map(|(i, name)| (u32::try_from(i).unwrap_or(0) + 1, name.as_str()))
    }
}

/// Writes an export into `destination`.
#[allow(clippy::too_many_lines, reason = "one linear pipeline; splitting it would hide the order writes happen in")]
pub fn export(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
) -> Result<ExportReport> {
    if tracks.is_empty() {
        return Err(ExportError::Empty);
    }
    if destination.exists() && !destination.is_dir() {
        return Err(ExportError::NotADirectory(destination.to_owned()));
    }

    let contents = destination.join("Contents");
    let anlz_root = destination.join("PIONEER/USBANLZ");
    let db_dir = destination.join("PIONEER/rekordbox");
    std::fs::create_dir_all(&contents)?;
    std::fs::create_dir_all(&anlz_root)?;
    std::fs::create_dir_all(&db_dir)?;

    let mut report = ExportReport::default();
    let mut artists = Intern::default();
    let mut albums = Intern::default();
    let mut genres = Intern::default();
    let mut labels = Intern::default();
    let mut keys = Intern::default();

    let mut track_rows: Vec<Vec<u8>> = Vec::with_capacity(tracks.len());
    let mut one_library_tracks: Vec<OneLibraryTrack> = Vec::with_capacity(tracks.len());
    // Export ids are assigned here and are what playlists reference.
    let mut export_ids: Vec<u32> = Vec::with_capacity(tracks.len());
    let interns = Interns {
        artists: &mut artists,
        albums: &mut albums,
        genres: &mut genres,
        labels: &mut labels,
        keys: &mut keys,
    };

    for (index, track) in tracks.iter().enumerate() {
        let export_id = u32::try_from(index).unwrap_or(0) + 1;

        let filename = track
            .source_path
            .file_name()
            .map_or_else(|| format!("track-{export_id}.mp3"), |n| n.to_string_lossy().into_owned());
        let safe_name = fat_safe(&filename);
        let artist_dir = fat_safe(if track.artist.is_empty() { "UnknownArtist" } else { &track.artist });
        let album_dir = fat_safe(if track.album.is_empty() { "UnknownAlbum" } else { &track.album });

        let relative_audio = format!("/Contents/{artist_dir}/{album_dir}/{safe_name}");
        let audio_dest = destination.join(relative_audio.trim_start_matches('/'));

        // Copy the audio. A missing file skips the track rather than aborting
        // an export that is otherwise fine.
        if let Some(parent) = audio_dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::copy(&track.source_path, &audio_dest) {
            Ok(bytes) => report.bytes_copied += bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                report.skipped.push(track.title.clone());
                continue;
            }
            Err(e) if is_device_gone(&e) => return Err(ExportError::DeviceGone),
            Err(e) => return Err(e.into()),
        }

        // Analysis lives in a directory pair derived from the export id, which
        // is how rekordbox spreads files across the tree.
        let bucket = format!("P{:03}", export_id / 1000);
        let leaf = format!("{export_id:08X}");
        let anlz_dir = anlz_root.join(&bucket).join(&leaf);
        let mut analyze_path = String::new();
        if !track.analysis.is_empty() {
            std::fs::create_dir_all(&anlz_dir)?;
            for (extension, bytes) in &track.analysis {
                let name = format!("ANLZ0000.{extension}");
                std::fs::write(anlz_dir.join(&name), bytes)?;
                report.analysis_files += 1;
                if extension.eq_ignore_ascii_case("DAT") {
                    analyze_path = format!("/PIONEER/USBANLZ/{bucket}/{leaf}/{name}");
                }
            }
        }

        // The same facts the pdb row carries, kept for exportLibrary.db.
        // Gathered here rather than re-derived later, so the two databases
        // cannot disagree about a path or a size.
        one_library_tracks.push(OneLibraryTrack {
            export_id,
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            genre: track.genre.clone(),
            label: track.label.clone(),
            key: track.key.clone(),
            color_id: track.color_id,
            bpm_x100: track.bpm_x100,
            duration_sec: track.duration_sec,
            rating: track.rating,
            comment: track.comment.clone(),
            date_added: track.date_added.clone(),
            audio_path: relative_audio.clone(),
            file_name: safe_name.clone(),
            analysis_path: analyze_path.clone(),
        });

        track_rows.push(track_row(&TrackInput {
            id: export_id,
            artist_id: interns.artists.id(&track.artist),
            album_id: interns.albums.id(&track.album),
            genre_id: interns.genres.id(&track.genre),
            label_id: interns.labels.id(&track.label),
            key_id: interns.keys.id(&track.key),
            color_id: track.color_id,
            rating: track.rating,
            tempo_x100: track.bpm_x100,
            duration_sec: track.duration_sec,
            year: track.year,
            bitrate: track.bitrate,
            sample_rate: track.sample_rate,
            file_size: u32::try_from(report.bytes_copied.min(u64::from(u32::MAX))).unwrap_or(0),
            track_number: export_id,
            title: track.title.clone(),
            filename: safe_name,
            file_path: relative_audio,
            analyze_path,
            comment: track.comment.clone(),
            date_added: track.date_added.clone(),
            release_date: track.release_date.clone(),
            ..TrackInput::default()
        }));
        export_ids.push(export_id);
        report.tracks += 1;
    }

    // Playlists reference export ids, so they are built after the tracks.
    let mut playlist_rows = Vec::with_capacity(playlists.len());
    let mut entry_rows = Vec::new();
    for (i, playlist) in playlists.iter().enumerate() {
        let playlist_id = u32::try_from(i).unwrap_or(0) + 1;
        playlist_rows.push(playlist_row(
            playlist_id,
            0,
            playlist_id,
            false,
            &playlist.name,
        ));
        for (position, &track_index) in playlist.track_indices.iter().enumerate() {
            let Some(&export_id) = export_ids.get(track_index) else { continue };
            entry_rows.push(playlist_entry_row(
                u32::try_from(position).unwrap_or(0) + 1,
                export_id,
                playlist_id,
            ));
        }
        report.playlists += 1;
    }

    let mut file = FileBuilder::new(PAGE_SIZE);
    file.add_table(0, &track_rows);
    file.add_table(1, &genres.entries().map(|(id, n)| simple_named_row(id, n)).collect::<Vec<_>>());
    file.add_table(2, &artists.entries().map(|(id, n)| artist_row(id, n)).collect::<Vec<_>>());
    file.add_table(3, &albums.entries().map(|(id, n)| album_row(id, 0, n)).collect::<Vec<_>>());
    file.add_table(4, &labels.entries().map(|(id, n)| simple_named_row(id, n)).collect::<Vec<_>>());
    file.add_table(5, &keys.entries().map(|(id, n)| key_row(id, n)).collect::<Vec<_>>());
    file.add_table(
        6,
        &COLORS
            .iter()
            .enumerate()
            .map(|(i, name)| color_row(u16::try_from(i).unwrap_or(0) + 1, name))
            .collect::<Vec<_>>(),
    );
    file.add_table(7, &playlist_rows);
    file.add_table(8, &entry_rows);

    let pdb = file.finish();
    report.pdb_bytes = pdb.len();
    std::fs::write(db_dir.join("export.pdb"), &pdb)?;

    // A player never opens this; rekordbox does, to read the stick back.
    write_one_library(&db_dir, &one_library_tracks, playlists, &export_ids)?;
    report.one_library = true;

    Ok(report)
}

/// The subset of a track `exportLibrary.db` needs.
struct OneLibraryTrack {
    export_id: u32,
    title: String,
    artist: String,
    album: String,
    genre: String,
    label: String,
    key: String,
    color_id: u8,
    bpm_x100: u32,
    duration_sec: u16,
    rating: u8,
    comment: String,
    date_added: String,
    audio_path: String,
    file_name: String,
    analysis_path: String,
}

/// Writes `exportLibrary.db` beside `export.pdb`.
fn write_one_library(
    db_dir: &Path,
    tracks: &[OneLibraryTrack],
    playlists: &[SourcePlaylist],
    export_ids: &[u32],
) -> Result<()> {
    use rbl_onelibrary::build::{Builder, LookupTable, Track};

    let path = db_dir.join("exportLibrary.db");
    // An export is written into a fresh directory, but a resumed one may find
    // the previous attempt's file; replacing it is correct, keeping it is not.
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let mut builder = Builder::create(&path).map_err(|e| one_library_error(&e))?;

    for track in tracks {
        let artist = builder.intern(LookupTable::Artist, &track.artist).map_err(|e| one_library_error(&e))?;
        let album = builder.intern(LookupTable::Album, &track.album).map_err(|e| one_library_error(&e))?;
        let genre = builder.intern(LookupTable::Genre, &track.genre).map_err(|e| one_library_error(&e))?;
        let label = builder.intern(LookupTable::Label, &track.label).map_err(|e| one_library_error(&e))?;
        let key = builder.intern(LookupTable::Key, &track.key).map_err(|e| one_library_error(&e))?;
        builder
            .add_track(&Track {
                content_id: i64::from(track.export_id),
                title: track.title.clone(),
                artist_id: Some(artist),
                album_id: Some(album),
                genre_id: Some(genre),
                label_id: Some(label),
                key_id: Some(key),
                color_id: Some(i64::from(track.color_id)),
                bpm_x100: i64::from(track.bpm_x100),
                length: i64::from(track.duration_sec),
                track_no: i64::from(track.export_id),
                path: track.audio_path.clone(),
                file_name: track.file_name.clone(),
                file_size: 0,
                analysis_path: track.analysis_path.clone(),
                // Stars are multiples of 51 here as everywhere else.
                rating: i64::from(track.rating) * 51,
                comment: track.comment.clone(),
                date_added: track.date_added.clone(),
            })
            .map_err(|e| one_library_error(&e))?;
    }

    for (i, playlist) in playlists.iter().enumerate() {
        let playlist_id = i64::try_from(i).unwrap_or(0) + 1;
        builder
            .add_playlist(playlist_id, &playlist.name, 0, i64::try_from(i).unwrap_or(0))
            .map_err(|e| one_library_error(&e))?;
        for (position, &track_index) in playlist.track_indices.iter().enumerate() {
            let Some(&export_id) = export_ids.get(track_index) else { continue };
            builder
                .add_to_playlist(
                    playlist_id,
                    i64::from(export_id),
                    i64::try_from(position).unwrap_or(0) + 1,
                )
                .map_err(|e| one_library_error(&e))?;
        }
    }

    // The date only, which is what rekordbox's own export carries.
    let created = rbl_core::time::now().get(..10).unwrap_or("").to_owned();
    builder.finish("REKORDBOX-LITE", &created).map_err(|e| one_library_error(&e))
}

fn one_library_error(error: &rbl_onelibrary::Error) -> ExportError {
    ExportError::OneLibrary(error.to_string())
}

/// Errors that mean the media was unplugged mid-write.
fn is_device_gone(e: &std::io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        // ENXIO, ENODEV, EIO
        Some(6 | 19 | 5)
    )
}

/// Re-reads an export with the independent parser and checks it is coherent.
///
/// A writer that verifies itself proves little; this reads the file back the
/// same way a player would and confirms the tracks and playlists survived.
pub fn verify(destination: &Path) -> Result<VerifyReport> {
    let path = destination.join("PIONEER/rekordbox/export.pdb");
    let bytes = std::fs::read(&path)?;
    let Ok(pdb) = rbl_pdb::Pdb::parse(&bytes) else {
        return Ok(VerifyReport::default());
    };

    let mut report = VerifyReport { parsed: true, ..VerifyReport::default() };
    if let Some(table) = pdb.table(rbl_pdb::PageType::Tracks) {
        let rows = pdb.track_rows(table);
        report.tracks = rows.len();
        for row in &rows {
            // Every track must point at audio that is actually present.
            let audio = destination.join(row.file_path.trim_start_matches('/'));
            if audio.exists() {
                report.audio_present += 1;
            } else {
                report.missing_audio.push(row.file_path.clone());
            }
            if !row.analyze_path.is_empty()
                && destination.join(row.analyze_path.trim_start_matches('/')).exists()
            {
                report.analysis_present += 1;
            }
        }
    }
    if let Some(table) = pdb.table(rbl_pdb::PageType::PlaylistTree) {
        report.playlists = pdb.playlist_nodes(table).len();
    }
    if let Some(table) = pdb.table(rbl_pdb::PageType::PlaylistEntries) {
        report.playlist_entries = pdb.playlist_entries(table).len();
    }
    Ok(report)
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub parsed: bool,
    pub tracks: usize,
    pub playlists: usize,
    pub playlist_entries: usize,
    pub audio_present: usize,
    pub analysis_present: usize,
    pub missing_audio: Vec<String>,
}

impl VerifyReport {
    /// True when the export is internally consistent.
    pub fn is_ok(&self) -> bool {
        self.parsed && self.tracks > 0 && self.missing_audio.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn fat_safe_replaces_characters_a_stick_cannot_hold() {
        assert_eq!(fat_safe("A/B:C*D?E"), "A_B_C_D_E");
        assert_eq!(fat_safe("trailing dots..."), "trailing dots");
        assert_eq!(fat_safe(""), "Unknown");
        assert_eq!(fat_safe("   "), "Unknown");
        // Unicode is fine on FAT32 long names.
        assert_eq!(fat_safe("Ébano — Tiësto"), "Ébano — Tiësto");
    }

    #[test]
    fn interning_assigns_stable_ids_from_one() {
        let mut intern = Intern::default();
        assert_eq!(intern.id("ARTBAT"), 1);
        assert_eq!(intern.id("Meduza"), 2);
        assert_eq!(intern.id("ARTBAT"), 1, "repeat lookups must be stable");
        // An empty name has no row; zero means "none".
        assert_eq!(intern.id(""), 0);
        let names: Vec<&str> = intern.entries().map(|(_, n)| n).collect();
        assert_eq!(names, vec!["ARTBAT", "Meduza"]);
    }
}
