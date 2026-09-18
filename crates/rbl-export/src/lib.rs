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

pub mod manifest;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub use manifest::{track_key, Manifest, ManifestTrack};

use rbl_pdb::build::FileBuilder;
use rbl_pdb::rows::{
    artwork_row,
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
    /// `djmdContent.ID`, or 0 for a file that is not in the library. This is
    /// how a sync recognises a track it has already written.
    pub id: u64,
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
    /// The track's artwork, where the library keeps it; `None` for none.
    pub artwork: Option<PathBuf>,
    /// The library's ids of the My Tags on the track.
    pub my_tags: Vec<u64>,
}

/// One My Tag of the library, to be listed on the stick: a category
/// (`attribute` 1, `parent` 0) or a tag under one (`attribute` 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMyTag {
    pub id: u64,
    pub seq: u32,
    pub name: String,
    pub attribute: u8,
    pub parent: u64,
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
    /// Artwork files written this run; four per image, as rekordbox writes.
    pub artwork_files: usize,
    pub pdb_bytes: usize,
    /// Tracks skipped because their audio was missing or unreadable.
    pub skipped: Vec<String>,
    /// Tracks whose audio was already on the stick, unchanged, and left alone.
    pub reused: usize,
    /// What not copying them saved.
    pub bytes_reused: u64,
    /// Tracks taken off the stick because the selection no longer holds them.
    pub removed: usize,
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

/// Where one track's files land on the stick.
///
/// Derived from the track and its export id alone, so the same track lands in
/// the same place on every sync and a second export can tell "already there"
/// from "moved".
struct Layout {
    /// Relative to the stick root, with the leading slash a pdb row carries.
    audio: String,
    /// The analysis directory, relative to the stick root.
    anlz_dir: String,
    file_name: String,
}

fn layout(track: &SourceTrack, export_id: u32) -> Layout {
    let on_disk = track
        .source_path
        .file_name()
        .map_or_else(|| format!("track-{export_id}.mp3"), |n| n.to_string_lossy().into_owned());
    let file_name = fat_safe(&on_disk);
    let artist_dir = fat_safe(if track.artist.is_empty() { "UnknownArtist" } else { &track.artist });
    let album_dir = fat_safe(if track.album.is_empty() { "UnknownAlbum" } else { &track.album });
    Layout {
        audio: format!("/Contents/{artist_dir}/{album_dir}/{file_name}"),
        // The two levels are how rekordbox spreads analysis across the tree
        // rather than putting a quarter of a million files in one directory.
        anlz_dir: format!("/PIONEER/USBANLZ/P{:03}/{export_id:08X}", export_id / 1000),
        file_name,
    }
}

/// Gives every track the id it had on this stick last time, and a fresh one
/// otherwise.
///
/// Ids must not shift between syncs: a deck caches artwork and waveforms
/// against them, and every playlist entry names one. Reassigning by position
/// would silently repoint half the stick after a single track was removed.
fn assign_ids(tracks: &[SourceTrack], previous: Option<&Manifest>) -> Vec<u32> {
    let mut known: BTreeMap<String, u32> = BTreeMap::new();
    let mut used: BTreeSet<u32> = BTreeSet::new();
    if let Some(manifest) = previous {
        for entry in &manifest.tracks {
            known.insert(entry.key(), entry.export_id);
            used.insert(entry.export_id);
        }
    }

    let mut ids = Vec::with_capacity(tracks.len());
    let mut next: u32 = 1;
    for track in tracks {
        let key = track_key(track.id, &track.source_path.to_string_lossy());
        let id = known.get(&key).copied().unwrap_or_else(|| {
            while used.contains(&next) {
                next = next.saturating_add(1);
            }
            next
        });
        used.insert(id);
        // The same track listed twice keeps one id rather than taking two.
        known.insert(key, id);
        ids.push(id);
    }
    ids
}

/// Resolves a stick-relative path against the destination.
fn under(destination: &Path, relative: &str) -> PathBuf {
    destination.join(relative.trim_start_matches('/'))
}

/// The source's size and modification time, which together decide whether a
/// copy can be skipped.
fn source_stamp(meta: &std::fs::Metadata) -> (u64, i64) {
    // Nanoseconds, not seconds: a re-encode that happens to land on the same
    // byte count within the same second would otherwise read as unchanged.
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0);
    (meta.len(), modified)
}

/// Whether two paths name the same file on disk.
///
/// A stick is FAT32 and a Mac's disk is case-insensitive by default, so
/// renaming an artist from TRIODE to Triode changes the path we write without
/// changing the file. Deleting "the old path" afterwards would delete the copy
/// just made, and the stick would name audio that is no longer there.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Removes something the export no longer references, and any directory it
/// leaves empty behind it.
///
/// Failures are ignored on purpose: a file that would not delete leaves the
/// stick untidy, and failing the whole export over it would be worse.
fn remove_under(destination: &Path, relative: &str, directory: bool) {
    if relative.is_empty() {
        return;
    }
    let path = under(destination, relative);
    if directory {
        let _ = std::fs::remove_dir_all(&path);
    } else {
        let _ = std::fs::remove_file(&path);
    }
    let mut current = path.parent().map(Path::to_path_buf);
    while let Some(dir) = current {
        // Stop at the stick root, and stop as soon as a directory still holds
        // something — `remove_dir` refuses a non-empty one, which is the test.
        if dir == destination || !dir.starts_with(destination) || std::fs::remove_dir(&dir).is_err() {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
}

/// Writes an export into `destination`.
///
/// Exporting to a stick that already holds one of ours is a sync, not a
/// rewrite: the manifest left by the previous run says what is already there,
/// and only what changed is copied. Tracks that have left the selection are
/// removed. Without a manifest — a fresh stick, or one rekordbox wrote —
/// everything is written.
pub fn export(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
) -> Result<ExportReport> {
    export_with(destination, tracks, playlists, None)
}

/// [`export`], with what a stick that holds no `exportLibrary.db` yet
/// starts from in place of the reference rows: the Preferences window's
/// DJ System choices. A stick that already has a library keeps its own
/// settings, and `defaults` is not looked at.
pub fn export_with(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    defaults: Option<&rbl_onelibrary::settings::StickSettings>,
) -> Result<ExportReport> {
    export_full(destination, tracks, playlists, &[], defaults)
}

/// [`export_with`], with the library's My Tags listed on the stick as well.
///
/// The categories and tags go to `exportLibrary.db` whole, as rekordbox
/// writes them (every live row of `djmdMyTag`, 99 on the reference library),
/// and each track's memberships with them; `export.pdb` has no My Tag
/// table, so a player filters by them only through the library file.
#[allow(clippy::too_many_lines, reason = "one linear pipeline; splitting it would hide the order writes happen in")]
pub fn export_full(
    destination: &Path,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    my_tags: &[SourceMyTag],
    defaults: Option<&rbl_onelibrary::settings::StickSettings>,
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

    let previous = Manifest::load(destination);
    let ids = assign_ids(tracks, previous.as_ref());
    // Entries are taken out as they are matched; whatever is left at the end
    // is what the selection no longer holds.
    let mut stale: BTreeMap<String, &ManifestTrack> = previous
        .as_ref()
        .map(|m| m.tracks.iter().map(|entry| (entry.key(), entry)).collect())
        .unwrap_or_default();

    let mut report = ExportReport::default();
    let mut artists = Intern::default();
    let mut albums = Intern::default();
    let mut genres = Intern::default();
    let mut labels = Intern::default();
    let mut keys = Intern::default();
    // Artwork by where it comes from: two tracks of one album share one
    // image, and the stick carries it once.
    let mut artwork = Intern::default();

    let mut track_rows: Vec<Vec<u8>> = Vec::with_capacity(tracks.len());
    let mut one_library_tracks: Vec<OneLibraryTrack> = Vec::with_capacity(tracks.len());
    // Indexed by position in `tracks`, so a skipped track does not shift the
    // ones after it out from under the playlists.
    let mut export_ids: Vec<Option<u32>> = vec![None; tracks.len()];
    let mut recorded: Vec<ManifestTrack> = Vec::with_capacity(tracks.len());
    let interns = Interns {
        artists: &mut artists,
        albums: &mut albums,
        genres: &mut genres,
        labels: &mut labels,
        keys: &mut keys,
    };

    for (index, track) in tracks.iter().enumerate() {
        let export_id = ids.get(index).copied().unwrap_or(0);
        let place = layout(track, export_id);
        let source = track.source_path.to_string_lossy().into_owned();
        let key = track_key(track.id, &source);

        // Read the source before claiming the previous entry: a track whose
        // audio has gone leaves its entry in `stale`, so the copy on the stick
        // is removed rather than orphaned by databases that no longer name it.
        let (size, modified) = match std::fs::metadata(&track.source_path) {
            Ok(meta) => source_stamp(&meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                report.skipped.push(track.title.clone());
                continue;
            }
            Err(e) if is_device_gone(&e) => return Err(ExportError::DeviceGone),
            Err(e) => return Err(e.into()),
        };

        let carried = stale.remove(&key);
        let audio_dest = under(destination, &place.audio);
        // Unchanged means: same source bytes by size and time, same place on
        // the stick, and still actually there.
        let unchanged = carried.is_some_and(|c| {
            c.audio == place.audio && c.size == size && c.modified == modified
        }) && audio_dest.exists();

        if unchanged {
            report.reused += 1;
            report.bytes_reused += size;
        } else {
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
        }

        // Renaming an artist moves the file; the copy under the old name would
        // otherwise sit on the stick forever, unreferenced.
        if let Some(c) = carried {
            if c.audio != place.audio && !same_file(&under(destination, &c.audio), &audio_dest) {
                remove_under(destination, &c.audio, false);
            }
            if c.anlz_dir != place.anlz_dir {
                remove_under(destination, &c.anlz_dir, true);
            }
        }

        let mut analysis_hash: u64 = 0;
        for (extension, bytes) in &track.analysis {
            analysis_hash = analysis_hash.rotate_left(7)
                ^ manifest::hash(extension.as_bytes())
                ^ manifest::hash(bytes);
        }
        let anlz_dir = under(destination, &place.anlz_dir);
        let analysis_current = carried
            .is_some_and(|c| c.anlz_dir == place.anlz_dir && c.analysis == analysis_hash)
            && track
                .analysis
                .iter()
                .all(|(extension, _)| anlz_dir.join(format!("ANLZ0000.{extension}")).exists());
        if !track.analysis.is_empty() && !analysis_current {
            std::fs::create_dir_all(&anlz_dir)?;
            for (extension, bytes) in &track.analysis {
                std::fs::write(anlz_dir.join(format!("ANLZ0000.{extension}")), bytes)?;
                report.analysis_files += 1;
            }
        }
        // The path the databases carry, whether or not the file was written
        // this run.
        let analyze_path = if track.analysis.iter().any(|(e, _)| e.eq_ignore_ascii_case("DAT")) {
            format!("{}/ANLZ0000.DAT", place.anlz_dir)
        } else {
            String::new()
        };

        // The artwork, written once per image: a second track of the same
        // album finds its id already taken and its files already there.
        let artwork_id = match track.artwork.as_deref().filter(|p| p.is_file()) {
            Some(image) => {
                let id = artwork.id(&image.to_string_lossy());
                report.artwork_files += write_artwork(destination, id, image)?;
                id
            }
            None => 0,
        };

        recorded.push(ManifestTrack {
            export_id,
            library_id: track.id,
            source,
            audio: place.audio.clone(),
            anlz_dir: if track.analysis.is_empty() { String::new() } else { place.anlz_dir.clone() },
            size,
            modified,
            analysis: analysis_hash,
            artwork: if artwork_id == 0 { String::new() } else { artwork_path(artwork_id, "a", false) },
        });

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
            audio_path: place.audio.clone(),
            file_name: place.file_name.clone(),
            analysis_path: analyze_path.clone(),
            image_id: artwork_id,
            my_tags: track.my_tags.clone(),
        });

        track_rows.push(track_row(&TrackInput {
            id: export_id,
            artwork_id,
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
            file_size: u32::try_from(size.min(u64::from(u32::MAX))).unwrap_or(0),
            track_number: export_id,
            title: track.title.clone(),
            filename: place.file_name,
            file_path: place.audio,
            analyze_path,
            comment: track.comment.clone(),
            date_added: track.date_added.clone(),
            release_date: track.release_date.clone(),
            ..TrackInput::default()
        }));
        if let Some(slot) = export_ids.get_mut(index) {
            *slot = Some(export_id);
        }
        report.tracks += 1;
    }

    // Whatever the previous export left that this one does not name.
    for entry in stale.values() {
        remove_under(destination, &entry.audio, false);
        remove_under(destination, &entry.anlz_dir, true);
        report.removed += 1;
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
        let mut position: u32 = 0;
        for &track_index in &playlist.track_indices {
            let Some(Some(export_id)) = export_ids.get(track_index).copied() else { continue };
            position += 1;
            entry_rows.push(playlist_entry_row(position, export_id, playlist_id));
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
    // The artwork table names the small image of each; a player derives the
    // others from the same name [ASSUME: what the `artwork` row of a
    // rekordbox export names, of the four files it writes per image].
    let artwork_paths: Vec<(u32, String)> =
        artwork.entries().map(|(id, _)| (id, artwork_path(id, "a", false))).collect();
    file.add_table(13, &artwork_paths.iter().map(|(id, path)| artwork_row(*id, path)).collect::<Vec<_>>());

    let pdb = file.finish();
    report.pdb_bytes = pdb.len();
    std::fs::write(db_dir.join("export.pdb"), &pdb)?;

    // A player never opens this; rekordbox does, to read the stick back.
    write_one_library(&db_dir, &one_library_tracks, playlists, &export_ids, &artwork_paths, my_tags, defaults)?;
    report.one_library = true;

    // Last, so a run that fails part way leaves the older record standing and
    // the next attempt re-copies rather than trusting a half-written stick.
    Manifest {
        version: manifest::MANIFEST_VERSION,
        written: rbl_core::time::now(),
        tracks: recorded,
    }
    .save(destination)?;

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
    /// The artwork's id in the `artwork` table and the `image` table; 0 none.
    image_id: u32,
    my_tags: Vec<u64>,
}

/// Where an image's files go on the stick, and what the databases name.
///
/// rekordbox writes four files per image — `a<id>.jpg`, `a<id>_m.jpg`,
/// `b<id>.jpg` and `b<id>_m.jpg` — a thousand images to a five-digit folder
/// [OBS, 2026-09-17 parity test]. Which size each is, and whether the
/// stick's copies are re-encoded from the library's, is not settled (they
/// did not match the library's files byte for byte), so every one is the
/// library's image as it is: a player that wants the small one gets a
/// larger one to scale [ASSUME].
fn artwork_path(id: u32, prefix: &str, medium: bool) -> String {
    let folder = (id.saturating_sub(1)) / 1000 + 1;
    let suffix = if medium { "_m" } else { "" };
    format!("/PIONEER/Artwork/{folder:05}/{prefix}{id}{suffix}.jpg")
}

/// The four names an image is written under.
fn artwork_names(id: u32) -> [String; 4] {
    [
        artwork_path(id, "a", false),
        artwork_path(id, "a", true),
        artwork_path(id, "b", false),
        artwork_path(id, "b", true),
    ]
}

/// Copies an image to its four places, skipping any already there at the
/// same size. Returns how many files were written.
fn write_artwork(destination: &Path, id: u32, image: &Path) -> Result<usize> {
    let size = std::fs::metadata(image)?.len();
    let mut written = 0;
    for name in artwork_names(id) {
        let target = under(destination, &name);
        if std::fs::metadata(&target).is_ok_and(|m| m.len() == size) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::copy(image, &target) {
            Ok(_) => written += 1,
            Err(e) if is_device_gone(&e) => return Err(ExportError::DeviceGone),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(written)
}

/// Writes `exportLibrary.db` beside `export.pdb`.
#[allow(clippy::too_many_arguments, reason = "the stick's tables, each from its own source")]
fn write_one_library(
    db_dir: &Path,
    tracks: &[OneLibraryTrack],
    playlists: &[SourcePlaylist],
    export_ids: &[Option<u32>],
    artwork_paths: &[(u32, String)],
    my_tags: &[SourceMyTag],
    defaults: Option<&rbl_onelibrary::settings::StickSettings>,
) -> Result<()> {
    use rbl_onelibrary::build::{Builder, LookupTable, Track};
    use rbl_onelibrary::settings::StickSettings;

    let path = db_dir.join("exportLibrary.db");
    // The database is rebuilt from scratch, but the settings the stick already
    // carries — its name, which browse categories and sorts are on, the colour
    // comments — are the user's and survive the rebuild. A stick that holds
    // none, or one that cannot be read, starts from the defaults it was
    // given, or from the reference rows.
    let fresh = || defaults.cloned().unwrap_or_default();
    let settings = if path.exists() {
        StickSettings::read(&path).unwrap_or_else(|_| fresh())
    } else {
        fresh()
    };
    // An export is written into a fresh directory, but a resumed one may find
    // the previous attempt's file; replacing it is correct, keeping it is not.
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let mut builder = Builder::create_with(&path, &settings).map_err(|e| one_library_error(&e))?;

    for (id, image) in artwork_paths {
        builder.add_image(i64::from(*id), image).map_err(|e| one_library_error(&e))?;
    }
    // Every tag the library has, whether or not a track here carries it,
    // which is what rekordbox lists; then only the memberships of tags that
    // exist, so a stale membership cannot point at nothing.
    let mut known_tags: BTreeSet<u64> = BTreeSet::new();
    for tag in my_tags {
        let id = i64::try_from(tag.id).unwrap_or(0);
        if id == 0 {
            continue;
        }
        builder
            .add_my_tag(
                id,
                i64::from(tag.seq),
                &tag.name,
                i64::from(tag.attribute),
                i64::try_from(tag.parent).unwrap_or(0),
            )
            .map_err(|e| one_library_error(&e))?;
        known_tags.insert(tag.id);
    }

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
                image_id: (track.image_id != 0).then_some(i64::from(track.image_id)),
            })
            .map_err(|e| one_library_error(&e))?;
        for tag in &track.my_tags {
            if !known_tags.contains(tag) {
                continue;
            }
            builder
                .tag_track(i64::try_from(*tag).unwrap_or(0), i64::from(track.export_id))
                .map_err(|e| one_library_error(&e))?;
        }
    }

    for (i, playlist) in playlists.iter().enumerate() {
        let playlist_id = i64::try_from(i).unwrap_or(0) + 1;
        builder
            .add_playlist(playlist_id, &playlist.name, 0, i64::try_from(i).unwrap_or(0))
            .map_err(|e| one_library_error(&e))?;
        let mut position: i64 = 0;
        for &track_index in &playlist.track_indices {
            let Some(Some(export_id)) = export_ids.get(track_index).copied() else { continue };
            position += 1;
            builder
                .add_to_playlist(playlist_id, i64::from(export_id), position)
                .map_err(|e| one_library_error(&e))?;
        }
    }

    // The date only, and the local one: rekordbox's own export carries the
    // day the person exported on, not the UTC day.
    let created = rbl_core::time::local_date();
    let device_name = if settings.device_name.is_empty() {
        "RBXPORT"
    } else {
        settings.device_name.as_str()
    };
    builder.finish(device_name, &created).map_err(|e| one_library_error(&e))
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
