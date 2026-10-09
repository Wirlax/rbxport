//! Organize Library: every track's file moved into one music folder, filed
//! as `<folder>/<Artist>/<Album>/<file name>`, and the library pointed at it.
//! This fork's own; rekordbox has no such command.
//!
//! A track's file is named in `djmdContent.FolderPath` and `FileNameL`, which
//! [`rbl_db::write::Writer::relocate`] rewrites; playlists, cues, history and
//! My Tags key off the track's id and do not move. The analysis files stay
//! where they are: the `PPTH` rekordbox writes in them holds `?/<file name>`
//! [OBS 2026-10-09, a user's library: 2,271 of 2,488 tracks], which is why
//! the file name is kept. Folder names are made the way a USB export makes
//! its `Contents/` ones ([`rbl_export::dir_name`]), so the folder can go to a
//! FAT stick or a cloud drive as it is.
//!
//! Left alone: cloud-shared and streamed tracks, whose file is the service's,
//! and rekordbox's own files under the Music folder, its sampler sounds and
//! demo tracks.
//!
//! Each move is journalled before it is made and marked done once the
//! library points at the new place. A run cut short is put right before the
//! next run or undo, and the last run can be undone.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbl_db::track_files::{folder_path, track_files, TrackFile};
use rbl_db::track_path::CLOUD_SHARED;
use rbl_db::write::Writer;
use rbl_db::DbError;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use unicode_normalization::UnicodeNormalization;

use crate::commands::{blocking, reload, write_error};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::AppState;

const UNKNOWN_ARTIST: &str = "Unknown Artist";
const UNKNOWN_ALBUM: &str = "Unknown Album";

/// One file to move, and every track that points at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub ids: Vec<String>,
    pub from: PathBuf,
    pub to: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub moves: Vec<Move>,
    /// Tracks whose file is already where it belongs.
    pub in_place: u32,
    /// Tracks whose file is not there to move.
    pub missing: u32,
    /// Cloud-shared, streamed and rekordbox's own tracks.
    pub left_alone: u32,
}

/// A path as compared: NFC and lower case, since macOS lists names
/// decomposed and its disks ignore case.
fn key(path: &Path) -> String {
    path.to_string_lossy().nfc().collect::<String>().to_lowercase()
}

fn destination(root: &Path, track: &TrackFile, file_name: &std::ffi::OsStr) -> PathBuf {
    let named = |name: &str, unknown: &str| rbl_export::dir_name(if name.trim().is_empty() { unknown } else { name });
    root.join(named(&track.artist, UNKNOWN_ARTIST)).join(named(&track.album, UNKNOWN_ALBUM)).join(file_name)
}

/// `wanted`, or `Name (2).ext` and so on: the first name neither taken by
/// this plan nor already on disk.
fn free_name(wanted: &Path, taken: &HashSet<String>) -> PathBuf {
    let stem = wanted.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = wanted.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut candidate = wanted.to_path_buf();
    let mut n = 1;
    // perf-ok: one `stat` per clash, under `blocking`.
    while taken.contains(&key(&candidate)) || candidate.exists() {
        n += 1;
        candidate = wanted.with_file_name(format!("{stem} ({n}){extension}"));
    }
    candidate
}

/// Where every track's file goes. `protected` are folders whose files are
/// left where they are.
pub fn plan(tracks: &[TrackFile], root: &Path, protected: &[PathBuf]) -> Plan {
    let mut plan = Plan::default();
    // One move per file, however many tracks point at it; the first track
    // (the lowest id) decides where it is filed.
    let mut files: Vec<(PathBuf, Vec<&TrackFile>)> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for track in tracks {
        let path = PathBuf::from(&track.folder_path);
        if track.service_id != 0
            || track.content_link & CLOUD_SHARED != 0
            || protected.iter().any(|folder| path.starts_with(folder))
        {
            plan.left_alone += 1;
            continue;
        }
        // perf-ok: one `stat` per track, under `blocking`.
        if !path.is_absolute() || !path.is_file() {
            plan.missing += 1;
            continue;
        }
        let at = *by_key.entry(key(&path)).or_insert_with(|| {
            files.push((path, Vec::new()));
            files.len() - 1
        });
        if let Some((_, owners)) = files.get_mut(at) {
            owners.push(track);
        }
    }

    // Files already in place keep their names, and newcomers go around them.
    let mut taken: HashSet<String> = HashSet::new();
    let mut pending: Vec<(PathBuf, Vec<&TrackFile>, PathBuf)> = Vec::new();
    for (path, owners) in files {
        let (Some(first), Some(name)) = (owners.first(), path.file_name()) else { continue };
        let wanted = destination(root, first, name);
        if key(&wanted) == key(&path) {
            plan.in_place += u32::try_from(owners.len()).unwrap_or(u32::MAX);
            taken.insert(key(&path));
        } else {
            pending.push((path, owners, wanted));
        }
    }
    for (from, owners, wanted) in pending {
        let to = free_name(&wanted, &taken);
        taken.insert(key(&to));
        let bytes = fs::metadata(&from).map_or(0, |m| m.len());
        plan.moves.push(Move { ids: owners.iter().map(|t| t.id.clone()).collect(), from, to, bytes });
    }
    plan
}

/// Moves a file, copying it across volumes, where a rename cannot.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(error) = fs::copy(from, to) {
                let _ = fs::remove_file(to);
                return Err(error);
            }
            fs::remove_file(from)
        }
        other => other,
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "step")]
enum Step {
    /// About to move `from` to `to`; the library still points at `from`.
    Start { from: PathBuf, to: PathBuf, ids: Vec<String> },
    /// The file is at `to` and the library points at it.
    Done,
    /// The move did not happen, or was put back.
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    Pending,
    Done,
    RolledBack,
}

#[derive(Debug)]
struct Entry {
    from: PathBuf,
    to: PathBuf,
    ids: Vec<String>,
    state: EntryState,
}

/// One run's journal: a line per step, appended as it happens.
struct Journal {
    path: PathBuf,
    file: Option<File>,
}

const JOURNAL_PREFIX: &str = "organize-";
const JOURNAL_SUFFIX: &str = ".jsonl";
const UNDONE_SUFFIX: &str = ".undone.jsonl";

impl Journal {
    /// A new run's journal, created with its first line, so a run that moves
    /// nothing leaves nothing behind.
    fn new(dir: &Path) -> Self {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self { path: dir.join(format!("{JOURNAL_PREFIX}{millis:020}{JOURNAL_SUFFIX}")), file: None }
    }

    fn existing(path: PathBuf) -> Self {
        Self { path, file: None }
    }

    fn write(&mut self, step: &Step) -> Result<(), DbError> {
        if self.file.is_none() {
            if let Some(dir) = self.path.parent() {
                fs::create_dir_all(dir)?;
            }
            self.file = Some(OpenOptions::new().create(true).append(true).open(&self.path)?);
        }
        let mut line = serde_json::to_vec(step).map_err(|e| DbError::Io(e.into()))?;
        line.push(b'\n');
        if let Some(file) = self.file.as_mut() {
            file.write_all(&line)?;
        }
        Ok(())
    }

    fn entries(&self) -> Result<Vec<Entry>, DbError> {
        let mut entries: Vec<Entry> = Vec::new();
        for line in BufReader::new(File::open(&self.path)?).lines() {
            // A line cut short by a crash is the last one, and says nothing.
            let Ok(step) = serde_json::from_str::<Step>(&line?) else { continue };
            match step {
                Step::Start { from, to, ids } => entries.push(Entry { from, to, ids, state: EntryState::Pending }),
                Step::Done | Step::RolledBack => {
                    if let Some(last) = entries.last_mut().filter(|e| e.state == EntryState::Pending) {
                        last.state = if matches!(step, Step::Done) { EntryState::Done } else { EntryState::RolledBack };
                    }
                }
            }
        }
        Ok(entries)
    }
}

/// The newest run's journal that has not been undone.
fn latest(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with(JOURNAL_PREFIX) && name.ends_with(JOURNAL_SUFFIX) && !name.ends_with(UNDONE_SUFFIX)
        })
        .max()
}

fn points_at(writer: &Writer, id: &str, path: &Path) -> Result<bool, DbError> {
    Ok(folder_path(writer.library().connection(), id)?.is_some_and(|stored| key(Path::new(&stored)) == key(path)))
}

/// Settles a move a run was cut short in: finished when the library already
/// points at the new place, put back otherwise.
pub fn recover(writer: &mut Writer, dir: &Path) -> Result<(), DbError> {
    let Some(path) = latest(dir) else { return Ok(()) };
    let mut journal = Journal::existing(path);
    let Some(last) = journal.entries()?.pop().filter(|e| e.state == EntryState::Pending) else { return Ok(()) };
    let mut moved = false;
    for id in &last.ids {
        moved |= points_at(writer, id, &last.to)?;
    }
    if moved && last.to.is_file() {
        for id in &last.ids {
            writer.relocate(id, &last.to)?;
        }
        return journal.write(&Step::Done);
    }
    if last.to.is_file() {
        if last.from.exists() {
            // A copy across volumes that never got as far as the library:
            // the original is still there, and the copy is ours.
            fs::remove_file(&last.to)?;
        } else {
            move_file(&last.to, &last.from)?;
        }
    }
    if last.from.is_file() {
        for id in &last.ids {
            if points_at(writer, id, &last.to)? {
                writer.relocate(id, &last.from)?;
            }
        }
    }
    journal.write(&Step::RolledBack)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub files: u32,
    pub tracks: u32,
    /// Files that could not be moved, with why; their tracks are unchanged.
    pub failed: Vec<String>,
}

/// Makes the plan's moves, one file at a time. A file that cannot be moved is
/// reported and passed over; a library write that fails puts that file back
/// and stops the run.
pub fn apply(
    writer: &mut Writer,
    plan: &Plan,
    dir: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Outcome, DbError> {
    let mut journal = Journal::new(dir);
    let mut outcome = Outcome::default();
    let total = plan.moves.len();
    for (done, step) in plan.moves.iter().enumerate() {
        progress(done, total);
        journal.write(&Step::Start { from: step.from.clone(), to: step.to.clone(), ids: step.ids.clone() })?;
        if let Err(error) = move_file(&step.from, &step.to) {
            journal.write(&Step::RolledBack)?;
            outcome.failed.push(format!("{}: {error}", step.from.display()));
            continue;
        }
        let pointed = step.ids.iter().try_for_each(|id| writer.relocate(id, &step.to).map(|_| ()));
        if let Err(error) = pointed {
            // The file goes back and the library with it. What cannot be put
            // back now stays pending, for `recover` to settle next time.
            let restored = move_file(&step.to, &step.from).is_ok()
                && step.ids.iter().all(|id| writer.relocate(id, &step.from).is_ok());
            if restored {
                journal.write(&Step::RolledBack)?;
            }
            return Err(error);
        }
        journal.write(&Step::Done)?;
        outcome.files += 1;
        outcome.tracks += u32::try_from(step.ids.len()).unwrap_or(u32::MAX);
    }
    progress(total, total);
    Ok(outcome)
}

/// What an undo did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Undone {
    pub files: u32,
    /// Files no longer where the run put them, left as they are.
    pub skipped: u32,
}

/// Puts the last run's files back and points the library at them again.
/// Safe to run again after being cut short: a file already back only has
/// its tracks pointed at it.
pub fn undo(writer: &mut Writer, dir: &Path) -> Result<Undone, DbError> {
    recover(writer, dir)?;
    let Some(path) = latest(dir) else { return Ok(Undone::default()) };
    let journal = Journal::existing(path.clone());
    let mut undone = Undone::default();
    for entry in journal.entries()?.iter().rev().filter(|e| e.state == EntryState::Done) {
        let back = if entry.from.is_file() && !entry.to.exists() {
            true
        } else if entry.to.is_file() && !entry.from.exists() {
            move_file(&entry.to, &entry.from).is_ok()
        } else {
            false
        };
        if !back {
            undone.skipped += 1;
            continue;
        }
        for id in &entry.ids {
            if points_at(writer, id, &entry.to)? {
                writer.relocate(id, &entry.from)?;
            }
        }
        undone.files += 1;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().replace(JOURNAL_SUFFIX, UNDONE_SUFFIX)).unwrap_or_default();
    fs::rename(&path, path.with_file_name(name))?;
    Ok(undone)
}

/// The last run that can be undone: when, and how many files it moved.
fn last_run(dir: &Path) -> Option<OrganizeLastDto> {
    let path = latest(dir)?;
    let entries = Journal::existing(path.clone()).entries().ok()?;
    let files = entries.iter().filter(|e| e.state == EntryState::Done).count();
    let at = path
        .file_name()?
        .to_string_lossy()
        .strip_prefix(JOURNAL_PREFIX)?
        .strip_suffix(JOURNAL_SUFFIX)?
        .parse::<u64>()
        .ok()?;
    (files > 0).then(|| OrganizeLastDto { at, files: u32::try_from(files).unwrap_or(u32::MAX) })
}

// ------------------------------------------------------------- the commands

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePreviewDto {
    pub files: u32,
    pub tracks: u32,
    pub bytes: u64,
    pub in_place: u32,
    pub missing: u32,
    pub left_alone: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeReportDto {
    pub files: u32,
    pub tracks: u32,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeUndoDto {
    pub files: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeLastDto {
    /// Unix milliseconds.
    pub at: u64,
    pub files: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ProgressDto {
    done: usize,
    total: usize,
}

/// How often the progress event goes out: often enough to move, rarely
/// enough that a run of thousands of small renames is not an event storm.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

fn checked_root(root: String) -> AppResult<PathBuf> {
    let root = PathBuf::from(root);
    // perf-ok: one `stat` per command.
    if root.is_absolute() && root.is_dir() {
        Ok(root)
    } else {
        Err(AppError::new(ErrorKind::NotFound, "Choose a music folder that exists."))
    }
}

/// rekordbox's own folders under the Music folder: its sampler sounds and
/// recordings, and its demo tracks.
fn protected<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<PathBuf> {
    app.path().audio_dir().map(|music| vec![music.join("rekordbox"), music.join("PioneerDJ")]).unwrap_or_default()
}

fn journals(state: &AppState) -> PathBuf {
    state.backup_dir().join("organize")
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What Organize Library would do, without doing it.
#[tauri::command]
pub async fn organize_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    root: String,
) -> AppResult<OrganizePreviewDto> {
    let root = checked_root(root)?;
    let protected = protected(&app);
    let state = Arc::clone(&state);
    blocking("organize_preview", move || {
        let tracks = state.read_db(|db| track_files(db.connection())).map_err(write_error)?;
        let plan = plan(&tracks, &root, &protected);
        Ok(OrganizePreviewDto {
            files: count(plan.moves.len()),
            tracks: count(plan.moves.iter().map(|m| m.ids.len()).sum()),
            bytes: plan.moves.iter().map(|m| m.bytes).sum(),
            in_place: plan.in_place,
            missing: plan.missing,
            left_alone: plan.left_alone,
        })
    })
    .await
}

/// Moves every track's file into `root` and points the library at it.
#[tauri::command]
pub async fn organize_library<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    root: String,
) -> AppResult<OrganizeReportDto> {
    let root = checked_root(root)?;
    let protected = protected(&app);
    let writing = Arc::clone(&state);
    let events = app.clone();
    let outcome = blocking("organize_library", move || {
        let dir = journals(&writing);
        writing
            .write(|writer| {
                recover(writer, &dir)?;
                let tracks = track_files(writer.library().connection())?;
                let plan = plan(&tracks, &root, &protected);
                let mut sent: Option<Instant> = None;
                apply(writer, &plan, &dir, &mut |done, total| {
                    if done == total || sent.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
                        sent = Some(Instant::now());
                        let _ = tauri::Emitter::emit(&events, "organize:progress", ProgressDto { done, total });
                    }
                })
            })
            .map_err(write_error)
    })
    .await?;
    if outcome.files > 0 {
        reload(app, Arc::clone(&state)).await?;
    }
    Ok(OrganizeReportDto { files: outcome.files, tracks: outcome.tracks, failed: outcome.failed })
}

/// Puts the last Organize Library's files back where they were.
#[tauri::command]
pub async fn undo_organize<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<OrganizeUndoDto> {
    let writing = Arc::clone(&state);
    let undone = blocking("undo_organize", move || {
        let dir = journals(&writing);
        writing.write(|writer| undo(writer, &dir)).map_err(write_error)
    })
    .await?;
    if undone.files > 0 {
        reload(app, Arc::clone(&state)).await?;
    }
    Ok(OrganizeUndoDto { files: undone.files, skipped: undone.skipped })
}

/// The last Organize Library that can be undone, if any.
#[tauri::command]
pub async fn last_organize(state: State<'_, Arc<AppState>>) -> AppResult<Option<OrganizeLastDto>> {
    let state = Arc::clone(&state);
    blocking("last_organize", move || Ok(last_run(&journals(&state)))).await
}

#[cfg(test)]
mod tests {
    // Every test builds its own library in a tempdir; `RBXPORT_TEST` makes
    // the writer refuse the real install as well.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use rbl_db::fixture::{self, track_id, Shape};

    struct Fixture {
        dir: tempfile::TempDir,
        writer: Writer,
    }

    impl Fixture {
        fn new(tracks: usize) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let location = fixture::build(dir.path(), Shape { tracks, ..Shape::default() }).unwrap();
            let writer = Writer::open(location, dir.path().join("backups")).unwrap();
            Self { dir, writer }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.dir.path().join(relative)
        }

        fn root(&self) -> PathBuf {
            let root = self.path("Music Folder");
            fs::create_dir_all(&root).unwrap();
            root
        }

        fn journals(&self) -> PathBuf {
            self.path("journals")
        }

        /// Puts track `index`'s file at `relative`, filed under `artist` and `album`.
        fn track(&mut self, index: usize, relative: &str, artist: &str, album: &str) -> PathBuf {
            let path = self.path(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("audio {index}")).unwrap();
            let id = track_id(index);
            self.writer.relocate(&id, &path).unwrap();
            let conn = self.writer.library().connection();
            let now = "2026-10-09 12:00:00.000 +00:00";
            for (table, column, name) in [("djmdArtist", "ArtistID", artist), ("djmdAlbum", "AlbumID", album)] {
                if name.is_empty() {
                    continue;
                }
                let row = format!("{table}-{index}");
                conn.execute(
                    &format!("INSERT INTO {table} (ID, Name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)"),
                    [row.as_str(), name, now],
                )
                .unwrap();
                conn.execute(&format!("UPDATE djmdContent SET {column} = ?1 WHERE ID = ?2"), [&row, &id]).unwrap();
            }
            path
        }

        fn stored(&self, index: usize) -> String {
            folder_path(self.writer.library().connection(), &track_id(index)).unwrap().unwrap()
        }

        fn plan(&self) -> Plan {
            let tracks = track_files(self.writer.library().connection()).unwrap();
            plan(&tracks, &self.root(), &[self.path("Music/rekordbox")])
        }

        fn organize(&mut self) -> Outcome {
            let plan = self.plan();
            let dir = self.journals();
            apply(&mut self.writer, &plan, &dir, &mut |_, _| {}).unwrap()
        }
    }

    /// Only the tracks a test gave a file to count; the rest have none.
    fn moved(plan: &Plan) -> Vec<(String, PathBuf)> {
        plan.moves.iter().map(|m| (m.ids.join(","), m.to.clone())).collect()
    }

    #[test]
    fn files_go_under_their_artist_and_album_with_their_own_names() {
        let mut f = Fixture::new(3);
        f.track(0, "Downloads/Dr. Fresch - Let's Go.mp3", "Dr. Fresch", "Let's Go");
        f.track(1, "Music/deemix/Bass House/Mercer - In my arms.mp3", "Mercer", "");
        f.track(2, "Desktop/untitled.wav", "", "");
        let root = f.root();
        assert_eq!(
            moved(&f.plan()),
            vec![
                (track_id(0), root.join("Dr. Fresch/Let's Go/Dr. Fresch - Let's Go.mp3")),
                (track_id(1), root.join("Mercer/Unknown Album/Mercer - In my arms.mp3")),
                (track_id(2), root.join("Unknown Artist/Unknown Album/untitled.wav")),
            ]
        );
    }

    #[test]
    fn organizing_moves_the_files_and_points_the_library_at_them() {
        let mut f = Fixture::new(2);
        let from = f.track(0, "Downloads/a.mp3", "Artist", "Album");
        let outcome = f.organize();
        let to = f.root().join("Artist/Album/a.mp3");
        assert_eq!(outcome, Outcome { files: 1, tracks: 1, failed: vec![] });
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "audio 0");
        assert_eq!(f.stored(0), to.to_string_lossy());
        // Done is done: a second run finds everything in place.
        let again = f.plan();
        assert_eq!(again.moves, vec![]);
        assert_eq!(again.in_place, 1);
    }

    #[test]
    fn a_name_already_taken_gets_a_number() {
        let mut f = Fixture::new(3);
        let root = f.root();
        f.track(0, "Music Folder/Artist/Album/song.mp3", "Artist", "Album");
        f.track(1, "Downloads/song.mp3", "Artist", "Album");
        f.track(2, "Desktop/song.mp3", "Artist", "Album");
        let plan = f.plan();
        assert_eq!(plan.in_place, 1);
        assert_eq!(
            moved(&plan),
            vec![
                (track_id(1), root.join("Artist/Album/song (2).mp3")),
                (track_id(2), root.join("Artist/Album/song (3).mp3")),
            ]
        );
    }

    #[test]
    fn a_file_on_disk_that_is_not_in_the_library_is_not_written_over() {
        let mut f = Fixture::new(1);
        let root = f.root();
        fs::create_dir_all(root.join("Artist/Album")).unwrap();
        fs::write(root.join("Artist/Album/song.mp3"), b"someone else's").unwrap();
        f.track(0, "Downloads/song.mp3", "Artist", "Album");
        f.organize();
        assert_eq!(fs::read(root.join("Artist/Album/song.mp3")).unwrap(), b"someone else's");
        assert!(root.join("Artist/Album/song (2).mp3").is_file());
    }

    #[test]
    fn a_retagged_track_inside_the_folder_is_filed_again() {
        let mut f = Fixture::new(1);
        let root = f.root();
        f.track(0, "Music Folder/Old Name/Album/song.mp3", "New Name", "Album");
        assert_eq!(moved(&f.plan()), vec![(track_id(0), root.join("New Name/Album/song.mp3"))]);
    }

    #[test]
    fn two_tracks_on_one_file_move_it_once() {
        let mut f = Fixture::new(2);
        let from = f.track(0, "Downloads/song.mp3", "Artist", "Album");
        f.writer.relocate(&track_id(1), &from).unwrap();
        let outcome = f.organize();
        assert_eq!((outcome.files, outcome.tracks), (1, 2));
        assert_eq!(f.stored(0), f.stored(1));
    }

    #[test]
    fn missing_cloud_and_rekordbox_files_are_left_alone() {
        let mut f = Fixture::new(4);
        let gone = f.track(0, "Downloads/gone.mp3", "A", "B");
        fs::remove_file(&gone).unwrap();
        f.track(1, "Music/rekordbox/Sampler/NOISE.wav", "", "");
        f.track(2, "Downloads/shared.mp3", "A", "B");
        let shared = CLOUD_SHARED.to_string();
        f.writer
            .library()
            .connection()
            .execute("UPDATE djmdContent SET ContentLink = ?1 WHERE ID = ?2", [shared.as_str(), &track_id(2)])
            .unwrap();
        f.track(3, "Downloads/kept.mp3", "A", "B");
        let plan = f.plan();
        assert_eq!(plan.left_alone, 2);
        // The fixture's other tracks have no file either.
        assert!(plan.missing >= 1);
        assert_eq!(moved(&plan), vec![(track_id(3), f.root().join("A/B/kept.mp3"))]);
    }

    #[test]
    fn undo_puts_the_files_and_the_library_back() {
        let mut f = Fixture::new(2);
        let first = f.track(0, "Downloads/a.mp3", "Artist", "Album");
        let second = f.track(1, "Desktop/b.mp3", "Artist", "Album");
        f.organize();
        let dir = f.journals();
        assert_eq!(last_run(&dir).map(|run| run.files), Some(2));

        let undone = undo(&mut f.writer, &dir).unwrap();
        assert_eq!(undone, Undone { files: 2, skipped: 0 });
        assert!(first.is_file() && second.is_file());
        assert_eq!(f.stored(0), first.to_string_lossy());
        assert_eq!(f.stored(1), second.to_string_lossy());
        assert!(last_run(&dir).is_none(), "an undone run cannot be undone again");
    }

    #[test]
    fn undo_passes_over_a_file_moved_since() {
        let mut f = Fixture::new(1);
        f.track(0, "Downloads/a.mp3", "Artist", "Album");
        f.organize();
        fs::remove_file(f.root().join("Artist/Album/a.mp3")).unwrap();
        let dir = f.journals();
        let undone = undo(&mut f.writer, &dir).unwrap();
        assert_eq!(undone, Undone { files: 0, skipped: 1 });
    }

    #[test]
    fn a_run_cut_short_after_the_move_is_put_back() {
        let mut f = Fixture::new(1);
        let from = f.track(0, "Downloads/a.mp3", "Artist", "Album");
        let to = f.root().join("Artist/Album/a.mp3");
        // What a crash between the rename and the library write leaves.
        let mut journal = Journal::new(&f.journals());
        journal.write(&Step::Start { from: from.clone(), to: to.clone(), ids: vec![track_id(0)] }).unwrap();
        move_file(&from, &to).unwrap();

        let dir = f.journals();
        recover(&mut f.writer, &dir).unwrap();
        assert!(from.is_file() && !to.exists());
        assert_eq!(f.stored(0), from.to_string_lossy());
        assert!(last_run(&f.journals()).is_none());
    }

    #[test]
    fn a_run_cut_short_after_the_library_write_is_finished() {
        let mut f = Fixture::new(1);
        let from = f.track(0, "Downloads/a.mp3", "Artist", "Album");
        let to = f.root().join("Artist/Album/a.mp3");
        let mut journal = Journal::new(&f.journals());
        journal.write(&Step::Start { from: from.clone(), to: to.clone(), ids: vec![track_id(0)] }).unwrap();
        move_file(&from, &to).unwrap();
        f.writer.relocate(&track_id(0), &to).unwrap();

        let dir = f.journals();
        recover(&mut f.writer, &dir).unwrap();
        assert!(to.is_file());
        assert_eq!(f.stored(0), to.to_string_lossy());
        assert_eq!(last_run(&f.journals()).map(|run| run.files), Some(1));
    }
}
