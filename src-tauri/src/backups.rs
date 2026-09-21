//! Explicit library snapshots. No work is done here on ordinary edits.
use crate::{
    dto::BackupDto,
    error::{AppError, AppResult},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupProgress {
    pub running: bool,
    pub phase: String,
    pub copied_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
    pub path: Option<String>,
    pub current_item: Option<String>,
}

/// Reserve the job before spawning, so requests from two windows cannot queue duplicates.
pub fn start(state: std::sync::Arc<AppState>) -> AppResult<()> {
    {
        let mut progress = state.backup_progress.lock();
        if progress.running {
            return Err(error("A backup is already running."));
        }
        *progress = BackupProgress { running: true, phase: "preparing".into(), ..Default::default() };
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut last_detail = std::time::Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            create_with_progress(&state, &mut |phase, copied_bytes, total_bytes, item| {
                let mut progress = state.backup_progress.lock();
                if progress.phase == "stopping" {
                    return Err(AppError::new(crate::error::ErrorKind::Cancelled, "Backup stopped."));
                }
                if progress.phase != phase || progress.current_item.is_none() || last_detail.elapsed() >= std::time::Duration::from_secs(1) {
                    progress.current_item = Some(item.to_owned());
                    last_detail = std::time::Instant::now();
                }
                progress.phase = phase.into();
                progress.copied_bytes = copied_bytes;
                progress.total_bytes = total_bytes;
                Ok(())
            })
        })).unwrap_or_else(|_| Err(error("Backup worker stopped unexpectedly.")));
        let mut progress = state.backup_progress.lock();
        progress.running = false;
        progress.current_item = None;
        match result {
            Ok(path) => { progress.phase = "complete".into(); progress.path = Some(path); }
            Err(e) if e.kind == crate::error::ErrorKind::Cancelled => { progress.phase = "cancelled".into(); }
            Err(e) => { progress.phase = "failed".into(); progress.error = Some(e.message); }
        }
    });
    Ok(())
}

pub fn cancel(state: &AppState) {
    let mut progress = state.backup_progress.lock();
    if progress.running { progress.phase = "stopping".into(); }
}

fn tree_size(path: &Path, check: &mut dyn FnMut() -> AppResult<()>) -> AppResult<u64> {
    check()?;
    let meta = fs::symlink_metadata(path).map_err(error)?;
    if meta.file_type().is_symlink() { return Err(error("Symbolic links are not supported in backups.")); }
    if meta.is_file() { return Ok(meta.len()); }
    if !meta.is_dir() { return Err(error("Unsupported file in backup.")); }
    let mut bytes = 0;
    for entry in fs::read_dir(path).map_err(error)? {
        bytes += tree_size(&entry.map_err(error)?.path(), check)?;
    }
    Ok(bytes)
}

fn error(e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Backup: {e}"))
}
fn millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
fn analysis(location: &rbl_db::LibraryLocation) -> PathBuf {
    location.share_root.join("PIONEER/USBANLZ")
}
fn artwork(location: &rbl_db::LibraryLocation) -> PathBuf {
    location.share_root.join("PIONEER/Artwork")
}
// Library selections live beside master.db, outside the SQL database.
pub const LIBRARY_FILES: [&str; 2] = ["masterPlaylists6.xml", "automixPlaylist6.xml"];

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    library: PathBuf,
    created_at: u64,
    bytes: u64,
    #[serde(default)]
    includes_artwork: bool,
    #[serde(default)]
    library_files: Vec<String>,
}

// Refuse symlinks, including within a tree, so a backup never follows files
// outside its library and deletion cannot traverse outside the backup folder.
fn copy(source: &Path, target: &Path) -> AppResult<u64> {
    copy_progress(source, target, &mut |_| Ok(()))
}
fn copy_progress(source: &Path, target: &Path, progress: &mut dyn FnMut(u64) -> AppResult<()>) -> AppResult<u64> {
    progress(0)?;
    let meta = fs::symlink_metadata(source).map_err(error)?;
    if meta.file_type().is_symlink() {
        return Err(error("Symbolic links are not supported in backups."));
    }
    if meta.is_dir() {
        fs::create_dir_all(target).map_err(error)?;
        let mut bytes = 0;
        for entry in fs::read_dir(source).map_err(error)? {
            let entry = entry.map_err(error)?;
            bytes += copy_progress(&entry.path(), &target.join(entry.file_name()), progress)?;
        }
        crate::durable::sync_dir(target).map_err(error)?;
        Ok(bytes)
    } else if meta.is_file() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(error)?;
        }
        let mut refused = None;
        let result = crate::backup_copy::copy_file(source, target, &mut |bytes| {
            progress(bytes).map_err(|e| {
                refused = Some(e);
                std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
            })
        });
        if let Some(error) = refused { return Err(error); }
        result.map_err(error)
    } else {
        Err(error("Unsupported file in backup."))
    }
}
fn remove(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            fs::remove_dir_all(path).map_err(error)
        }
        Ok(_) => fs::remove_file(path).map_err(error),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}
fn writable(location: &rbl_db::LibraryLocation) -> AppResult<()> {
    if let Some(reason) = rbl_db::write_refusal_reason(
        location.is_real_install,
        std::env::var_os("RB_LITE_TEST").is_some(),
        rbl_db::is_rekordbox_running(),
    ) {
        return Err(error(reason));
    }
    Ok(())
}
fn checked(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let root = root.canonicalize().map_err(error)?;
    let meta = fs::symlink_metadata(path).map_err(error)?;
    if meta.file_type().is_symlink() {
        return Err(error("Not a managed backup."));
    }
    let path = path.canonicalize().map_err(error)?;
    if path.parent() != Some(root.as_path()) {
        return Err(error("Not a managed backup."));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !(name.starts_with("rbxport-backup-") && meta.is_file() && is_zip(&path)
        || name.starts_with("library-") && (meta.is_dir() || (meta.is_file() && path.extension().is_some_and(|e| e == "zip")))
        || name.starts_with("master-")
            && path.extension().is_some_and(|e| e == "db")
            && meta.is_file())
    {
        return Err(error("Not a managed backup."));
    }
    Ok(path)
}
fn manifest(path: &Path, location: &rbl_db::LibraryLocation) -> AppResult<Manifest> {
    let bytes = if path.is_dir() { fs::read(path.join("manifest.json")).map_err(error)? }
        else { crate::backup_zip::manifest(path).map_err(error)? };
    let saved: Manifest = serde_json::from_slice(&bytes).map_err(error)?;
    if saved.library_files.iter().any(|name| !LIBRARY_FILES.contains(&name.as_str())) {
        return Err(error("Unsupported library file in backup."));
    }
    if !matches!(saved.version, 1 | 2) || saved.library != location.master_db {
        return Err(error(
            "This backup belongs to another library or an unsupported version.",
        ));
    }
    Ok(saved)
}

fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn checked_archive(path: &Path) -> AppResult<PathBuf> {
    let meta = fs::symlink_metadata(path).map_err(error)?;
    if !meta.is_file() || meta.file_type().is_symlink() || !is_zip(path) {
        return Err(error("Choose an RBXport backup ZIP file."));
    }
    path.canonicalize().map_err(error)
}

/// Inspect a user-selected archive before the UI asks to replace the library.
/// Restoring validates it again, including every extracted entry and the DB.
pub fn inspect_archive(state: &AppState, path: &Path) -> AppResult<BackupDto> {
    let _gate = state.edit_gate.lock();
    let path = checked_archive(path)?;
    let saved = manifest(&path, &state.location()?)?;
    Ok(BackupDto {
        name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes: fs::metadata(&path).map_err(error)?.len(),
        created_at: saved.created_at,
        includes_analysis: true,
        includes_artwork: saved.includes_artwork,
    })
}

pub fn list(state: &AppState) -> AppResult<Vec<BackupDto>> {
    let _gate = state.edit_gate.lock();
    let location = state.location()?;
    let mut result = Vec::new();
    let entries = match fs::read_dir(state.backup_destination()) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(e) => return Err(error(e)),
    };
    for entry in entries {
        let path = entry.map_err(error)?.path();
        let Ok(path) = checked(&state.backup_destination(), &path) else {
            continue;
        };
        let (created_at, bytes, includes_analysis, includes_artwork) = if path.is_dir() || path.extension().is_some_and(|e| e == "zip") {
            let Ok(saved) = manifest(&path, &location) else {
                continue;
            };
            (saved.created_at, if path.is_file() { fs::metadata(&path).map_err(error)?.len() } else { saved.bytes }, true, saved.includes_artwork)
        } else {
            let meta = fs::metadata(&path).map_err(error)?;
            let created = meta
                .modified()
                .map_err(error)?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(0);
            let bytes = meta.len()
                + ["-wal", "-shm"]
                    .iter()
                    .filter_map(|suffix| fs::metadata(sidecar(&path, suffix)).ok())
                    .map(|m| m.len())
                    .sum::<u64>();
            (created, bytes, false, false)
        };
        result.push(BackupDto {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            bytes,
            created_at,
            includes_analysis,
            includes_artwork,
        });
    }
    result.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(result)
}

pub fn create(state: &AppState) -> AppResult<String> {
    create_with_progress(state, &mut |_, _, _, _| Ok(()))
}

pub fn validate_destination(directory: &Path, location: &rbl_db::LibraryLocation) -> AppResult<()> {
    for source in [analysis(location), artwork(location)] {
        if source.canonicalize().is_ok_and(|source| directory.starts_with(source)) {
            return Err(error("Choose a backup folder outside the library's analysis and artwork folders."));
        }
    }
    Ok(())
}

fn create_with_progress(state: &AppState, progress: &mut dyn FnMut(&str, u64, u64, &str) -> AppResult<()>) -> AppResult<String> {
    let _gate = state.edit_gate.lock();
    let _files = state.analysis_write.lock();
    let location = state.location()?;
    writable(&location)?;
    if pending(state.backup_dir()) {
        state.with_closed_reader(|| recover(state.backup_dir(), &location))?;
    }
    crate::file_journal::recover(state.backup_dir(), &location)?;
    let tree = analysis(&location);
    let art = artwork(&location);
    let wal = sidecar(&location.master_db, "-wal");
    let destination = state.backup_destination();
    let root = destination.as_path();
    crate::durable::create_dir_all(root).map_err(error)?;
    validate_destination(&root.canonicalize().map_err(error)?, &location)?;
    let created_at = millis();
    let id = uuid::Uuid::new_v4();
    let partial = root.join(format!(".partial-{id}"));
    let target = root.join(format!("rbxport-backup-{}.zip", rbl_core::time::local_backup_stamp()));
    if target.try_exists().map_err(error)? {
        return Err(error("A backup for this minute already exists. Try again in the next minute."));
    }
    let archive = root.join(format!(".partial-{id}.zip"));
    let result = (|| {
        fs::create_dir(&partial).map_err(error)?;
        let mut refused = None;
        let plan = if tree.exists() {
            let prepared = crate::backup_copy::TreeCopyPlan::prepare(&tree, &partial.join("analysis"), &mut |_| {
                progress("preparing", 0, 0, "Scanning analysis files").map_err(|e| {
                    refused = Some(e);
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
                })
            });
            if let Some(error) = refused { return Err(error); }
            Some(prepared.map_err(error)?)
        } else { None };
        let art_plan = if art.exists() {
            let prepared = crate::backup_copy::TreeCopyPlan::prepare(&art, &partial.join("artwork"), &mut |_| {
                progress("preparing", 0, 0, "Scanning artwork thumbnails").map_err(|e| {
                    refused = Some(e);
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
                })
            });
            if let Some(error) = refused { return Err(error); }
            Some(prepared.map_err(error)?)
        } else { None };
        let mut check = || progress("preparing", 0, 0, "Measuring database files");
        let library_root = location.master_db.parent().ok_or_else(|| error("Invalid database path"))?;
        let library_files: Vec<_> = LIBRARY_FILES.into_iter().filter(|name| library_root.join(name).exists()).collect();
        let mut library_bytes = 0;
        for name in &library_files {
            library_bytes += tree_size(&library_root.join(name), &mut check)?;
        }
        let total = tree_size(&location.master_db, &mut check)?
            + library_bytes
            + if wal.exists() { tree_size(&wal, &mut check)? } else { 0 }
            + plan.as_ref().map_or(0, |plan| plan.bytes)
            + art_plan.as_ref().map_or(0, |plan| plan.bytes);
        progress("copying", 0, total, "Database · master.db")?;
        let mut copied = 0;
        let mut copied_file = |bytes, source: Option<&Path>| {
            copied += bytes;
            let item = match source {
                Some(path) if path.starts_with(&art) => format!("Artwork thumbnails · Artwork/{}", path.strip_prefix(&art).unwrap_or(path).to_string_lossy().replace('\\', "/")),
                Some(path) if path.starts_with(&tree) => format!("Analysis files · USBANLZ/{}", path.strip_prefix(&tree).unwrap_or(path).to_string_lossy().replace('\\', "/")),
                Some(path) => format!("Database · {}", path.file_name().unwrap_or_default().to_string_lossy()),
                None => "Analysis files".to_owned(),
            };
            progress("copying", copied, total, &item)
        };
        let snapshot = rbl_db::Library::open(location.clone(), rbl_db::OpenMode::ReadOnly).map_err(error)?;
        snapshot.connection().execute("VACUUM main INTO ?1", [partial.join("master.db").to_string_lossy().as_ref()]).map_err(error)?;
        drop(snapshot);
        let mut bytes = fs::metadata(partial.join("master.db")).map_err(error)?.len();
        fs::File::open(partial.join("master.db")).map_err(error)?.sync_all().map_err(error)?;
        copied_file(bytes, Some(&location.master_db))?;
        for (plan, directory) in [(plan, "analysis"), (art_plan, "artwork")] {
        if let Some(plan) = plan {
            let mut refused = None;
            let copied = plan.compress(&mut |bytes, source| {
                copied_file(bytes, source).map_err(|e| {
                    refused = Some(e);
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
                })
            });
            if let Some(error) = refused { return Err(error); }
            bytes += copied.map_err(error)?;
        } else {
            fs::create_dir(partial.join(directory)).map_err(error)?;
        }
        }
        progress("validating", bytes, total, "Checking database · master.db")?;
        validate_database(&partial.join("master.db"), &location)?;
        remove(&partial.join("master.db-shm"))?;
        for name in &library_files {
            let source = library_root.join(name);
            let target = partial.join(name);
            let mut refused = None;
            let compressed = crate::backup_zip::compress_file(&source, &target, &mut |delta| {
                bytes += delta;
                progress("copying", bytes, total, &format!("Library settings · {name}")).map_err(|e| {
                    refused = Some(e);
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
                })
            });
            if let Some(error) = refused { return Err(error); }
            compressed.map_err(error)?;
        }
        for name in ["master.db", "master.db-wal"] {
            let source = partial.join(name);
            if !source.exists() { continue; }
            let mut refused = None;
            let compressed = crate::backup_zip::compress_file(&source, &source, &mut |_| {
                progress("compressing", bytes, total, &format!("Compressing database · {name}")).map_err(|e| {
                    refused = Some(e);
                    std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
                })
            });
            if let Some(error) = refused { return Err(error); }
            compressed.map_err(error)?;
            remove(&source)?;
        }
        let saved = Manifest {
            version: 2,
            library: location.master_db.clone(),
            created_at,
            bytes,
            includes_artwork: true,
            library_files: library_files.into_iter().map(str::to_owned).collect(),
        };
        crate::durable::write(
            &partial.join("manifest.json"),
            &serde_json::to_vec(&saved).map_err(error)?,
        )
        .map_err(error)?;
        progress("validating", bytes, total, "Finishing backup · manifest.json")?;
        let mut refused = None;
        let packed = crate::backup_zip::assemble(&partial, &archive, &mut || {
            progress("compressing", bytes, total, "Finishing ZIP archive").map_err(|e| {
                refused = Some(e);
                std::io::Error::new(std::io::ErrorKind::Interrupted, "Backup stopped")
            })
        });
        if let Some(error) = refused { return Err(error); }
        packed.map_err(error)?;
        remove(&partial)?;
        tempfile::TempPath::try_from_path(&archive).map_err(error)?
            .persist_noclobber(&target).map_err(error)?;
        crate::durable::sync_dir(root).map_err(error)?;
        Ok(target.to_string_lossy().into_owned())
    })();
    if result.is_err() {
        let _ = remove(&partial);
        let _ = remove(&archive);
    }
    result
}
fn validate_database(path: &Path, location: &rbl_db::LibraryLocation) -> AppResult<()> {
    let mut copy = location.clone();
    copy.master_db = path.to_path_buf();
    copy.is_real_install = false;
    let db = rbl_db::Library::open(copy, rbl_db::OpenMode::ReadOnly).map_err(error)?;
    let check: String = db
        .connection()
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(error)?;
    if check != "ok" {
        return Err(error("The backup database is damaged."));
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Swap {
    target: PathBuf,
    staged: PathBuf,
    previous: PathBuf,
    existed: bool,
}
#[derive(Serialize, Deserialize)]
struct Restore {
    library: PathBuf,
    committed: bool,
    swaps: Vec<Swap>,
}
fn journal(root: &Path) -> PathBuf {
    root.join("backup-restore.json")
}

pub fn pending(root: &Path) -> bool {
    journal(root).exists()
}
fn save_restore(root: &Path, restore: &Restore) -> AppResult<()> {
    crate::durable::write(&journal(root), &serde_json::to_vec(restore).map_err(error)?)
        .map_err(error)
}
/// A pending restore rolls back on restart; a committed one only needs cleanup.
pub fn recover(root: &Path, location: &rbl_db::LibraryLocation) -> AppResult<()> {
    let bytes = match fs::read(journal(root)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(error(e)),
    };
    let restore: Restore = serde_json::from_slice(&bytes).map_err(error)?;
    if restore.library != location.master_db {
        return Err(error("An interrupted restore belongs to another library."));
    }
    writable(location)?;
    let allowed = [
        &location.master_db,
        &sidecar(&location.master_db, "-wal"),
        &sidecar(&location.master_db, "-shm"),
        &analysis(location),
        &artwork(location),
    ];
    for swap in &restore.swaps {
        let library_file = LIBRARY_FILES.iter().any(|name| location.master_db.parent().is_some_and(|root| root.join(name) == swap.target));
        if (!allowed.contains(&&swap.target) && !library_file)
            || swap.staged.parent() != swap.target.parent()
            || swap.previous.parent() != swap.target.parent()
            || !swap
                .staged
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".rbxport-restore-"))
            || !swap
                .previous
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".rbxport-restore-"))
        {
            return Err(error("Invalid restore recovery paths."));
        }
    }
    for swap in restore.swaps.iter().rev() {
        if !restore.committed {
            if swap.previous.exists() {
                remove(&swap.target)?;
                fs::rename(&swap.previous, &swap.target).map_err(error)?;
            } else if !swap.existed && !swap.staged.exists() {
                remove(&swap.target)?;
            }
        }
        remove(&swap.staged)?;
        remove(&swap.previous)?;
        crate::durable::sync_dir(
            swap.target
                .parent()
                .ok_or_else(|| error("Invalid restore path"))?,
        )
        .map_err(error)?;
    }
    remove(&journal(root))?;
    crate::durable::sync_dir(root).map_err(error)
}

fn validate_staged_database(
    restore: &Restore,
    location: &rbl_db::LibraryLocation,
) -> AppResult<()> {
    let staged_db = &restore.swaps[0].staged;
    let staged_wal = &restore.swaps[1].staged;
    let check_wal = sidecar(staged_db, "-wal");
    let valid = (|| {
        if staged_wal.exists() {
            copy(staged_wal, &check_wal)?;
        }
        validate_database(staged_db, location)
    })();
    remove(&check_wal)?;
    remove(&sidecar(staged_db, "-shm"))?;
    valid
}

pub fn restore(state: &AppState, path: &Path) -> AppResult<()> {
    let _gate = state.edit_gate.lock();
    let _files = state.analysis_write.lock();
    let location = state.location()?;
    writable(&location)?;
    if state.link_running() {
        return Err(error("Turn off PRO DJ LINK before restoring the library."));
    }
    let path = if is_zip(path) { checked_archive(path)? } else { checked(&state.backup_destination(), path)? };
    let unpacked = if is_zip(&path) {
        manifest(&path, &location)?;
        crate::durable::create_dir_all(state.backup_dir()).map_err(error)?;
        Some(crate::backup_zip::extract(&path, state.backup_dir()).map_err(error)?)
    } else { None };
    let path = unpacked.as_ref().map_or(path, |value| value.0.clone());
    let (includes_artwork, library_files) = if path.is_dir() {
        let saved = manifest(&path, &location)?;
        if !path.join("analysis").is_dir() {
            return Err(error("The backup analysis folder is missing."));
        }
        if saved.includes_artwork && !path.join("artwork").is_dir() {
            return Err(error("The backup artwork folder is missing."));
        }
        for name in &saved.library_files {
            if !path.join(name).is_file() {
                return Err(error("A backup library settings file is missing."));
            }
        }
        (saved.includes_artwork, saved.library_files)
    } else { (false, Vec::new()) };
    state.with_closed_reader(|| {
        recover(state.backup_dir(), &location)?;
        crate::file_journal::recover(state.backup_dir(), &location)?;
        let database = if path.is_dir() {
            path.join("master.db")
        } else {
            path.clone()
        };
        let mut sources = vec![
            (location.master_db.clone(), Some(database.clone())),
            (
                sidecar(&location.master_db, "-wal"),
                Some(sidecar(&database, "-wal")),
            ),
            (sidecar(&location.master_db, "-shm"), None),
        ];
        if path.is_dir() {
            sources.push((analysis(&location), Some(path.join("analysis"))));
        }
        if includes_artwork {
            sources.push((artwork(&location), Some(path.join("artwork"))));
        }
        for name in &library_files {
            let root = location.master_db.parent().ok_or_else(|| error("Invalid database path"))?;
            sources.push((root.join(name), Some(path.join(name))));
        }
        let mut restore = Restore {
            library: location.master_db.clone(),
            committed: false,
            swaps: Vec::new(),
        };
        let result = (|| {
            for (target, source) in sources {
                let parent = target
                    .parent()
                    .ok_or_else(|| error("Invalid restore path"))?;
                crate::durable::create_dir_all(parent).map_err(error)?;
                let id = uuid::Uuid::new_v4();
                let staged = parent.join(format!(".rbxport-restore-{id}-new"));
                let previous = parent.join(format!(".rbxport-restore-{id}-old"));
                restore.swaps.push(Swap {
                    existed: target.exists(),
                    target,
                    staged,
                    previous,
                });
                let swap = restore
                    .swaps
                    .last()
                    .ok_or_else(|| error("Missing restore stage"))?;
                if let Some(source) = source.filter(|p| p.exists()) {
                    copy(&source, &swap.staged)?;
                }
            }
            validate_staged_database(&restore, &location)?;
            save_restore(state.backup_dir(), &restore)?;
            for swap in &restore.swaps {
                if swap.existed {
                    fs::rename(&swap.target, &swap.previous).map_err(error)?;
                }
                if swap.staged.exists() {
                    fs::rename(&swap.staged, &swap.target).map_err(error)?;
                }
                crate::durable::sync_dir(
                    swap.target
                        .parent()
                        .ok_or_else(|| error("Invalid restore path"))?,
                )
                .map_err(error)?;
            }
            restore.committed = true;
            save_restore(state.backup_dir(), &restore)?;
            recover(state.backup_dir(), &location)
        })();
        if result.is_err() {
            if journal(state.backup_dir()).exists() {
                recover(state.backup_dir(), &location)?;
            } else {
                for swap in &restore.swaps {
                    remove(&swap.staged)?;
                }
            }
        }
        result
    })
}

pub fn delete(state: &AppState, path: &Path) -> AppResult<()> {
    let _gate = state.edit_gate.lock();
    let path = checked(&state.backup_destination(), path)?;
    if path.is_dir() || path.extension().is_some_and(|e| e == "zip") {
        manifest(&path, &state.location()?)?;
    } else {
        for suffix in ["-wal", "-shm"] {
            remove(&sidecar(&path, suffix))?;
        }
    }
    remove(&path)?;
    crate::durable::sync_dir(&state.backup_destination()).map_err(error)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    fn fixture() -> (tempfile::TempDir, AppState, rbl_db::LibraryLocation) {
        let dir = tempfile::tempdir().unwrap();
        let location =
            rbl_db::fixture::build(dir.path(), rbl_db::fixture::Shape::default()).unwrap();
        let db = rbl_db::Library::open(location.clone(), rbl_db::OpenMode::ReadOnly).unwrap();
        let (library, _) = rbl_index::load(&db).unwrap();
        let state = AppState::with_backups(dir.path().join("backups"));
        state.set_library(library, false, db.schema().db_version, 0, location.clone());
        (dir, state, location)
    }
    #[test]
    fn editing_is_allowed_without_a_backup_and_after_the_last_is_deleted() {
        let (_dir, state, _location) = fixture();
        let track = rbl_db::fixture::track_id(1);
        state.write(|w| w.set_rating(&track, 4)).unwrap();
        assert_eq!(rating(&state), 4);
        let path = PathBuf::from(create(&state).unwrap());
        delete(&state, &path).unwrap();
        state.write(|w| w.set_rating(&track, 2)).unwrap();
        assert_eq!(rating(&state), 2);
    }

    #[test]
    fn filenames_use_local_24_hour_time_and_never_overwrite_the_same_minute() {
        let (_dir, state, _) = fixture();
        let before = rbl_core::time::local_backup_stamp();
        let first = PathBuf::from(create(&state).unwrap());
        let after = rbl_core::time::local_backup_stamp();
        let name = first.file_name().unwrap().to_string_lossy();
        assert!(name == format!("rbxport-backup-{before}.zip") || name == format!("rbxport-backup-{after}.zip"));
        let bytes = fs::read(&first).unwrap();
        // Reserve the current minute, including if the first copy crossed a boundary.
        let reserved = state.backup_destination().join(format!("rbxport-backup-{after}.zip"));
        if reserved != first { fs::copy(&first, &reserved).unwrap(); }
        assert!(create(&state).is_err());
        assert_eq!(fs::read(first).unwrap(), bytes);
        assert_eq!(fs::read(reserved).unwrap(), bytes);
    }

    #[test]
    fn moved_zip_restores_from_another_folder_even_after_being_renamed() {
        let (dir, state, _location) = fixture();
        let track = rbl_db::fixture::track_id(1);
        state.write(|w| w.set_rating(&track, 4)).unwrap();
        let original = PathBuf::from(create(&state).unwrap());
        let bytes = fs::read(&original).unwrap();
        let destination = dir.path().join("external drive");
        fs::create_dir(&destination).unwrap();
        let moved = destination.join(original.file_name().unwrap());
        fs::rename(&original, &moved).unwrap();
        assert!(!original.exists());
        assert_eq!(fs::read(&moved).unwrap(), bytes);
        assert!(list(&state).unwrap().is_empty());
        let renamed = destination.join("My saved library.ZIP");
        fs::rename(moved, &renamed).unwrap();
        assert_eq!(inspect_archive(&state, &renamed).unwrap().name, "My saved library.ZIP");
        state.write(|w| w.set_rating(&track, 1)).unwrap();
        fs::remove_dir_all(state.backup_dir()).unwrap();
        restore(&state, &renamed).unwrap();
        assert_eq!(rating(&state), 4);
        assert_eq!(fs::read(&renamed).unwrap(), bytes);
        assert!(delete(&state, &renamed).is_err());
        assert!(renamed.exists());
    }

    #[test]
    fn invalid_external_zip_is_rejected_without_changing_the_library() {
        let (dir, state, _location) = fixture();
        let before = rating(&state);
        let bad = dir.path().join("not a backup.zip");
        fs::write(&bad, b"not a ZIP").unwrap();
        assert!(inspect_archive(&state, &bad).is_err());
        assert!(restore(&state, &bad).is_err());
        assert_eq!(rating(&state), before);
        let (_other_dir, other, _) = fixture();
        let other_backup = create(&other).unwrap();
        assert!(inspect_archive(&state, Path::new(&other_backup)).is_err());
        assert!(restore(&state, Path::new(&other_backup)).is_err());
        assert_eq!(rating(&state), before);
    }

    #[test]
    fn default_destination_persists_and_only_new_backups_use_it() {
        let (dir, state, location) = fixture();
        let original = PathBuf::from(create(&state).unwrap());
        let destination = dir.path().join("new backup folder");
        fs::create_dir(&destination).unwrap();
        let canonical = destination.canonicalize().unwrap();
        state.set_backup_destination(&destination).unwrap();
        assert_eq!(state.backup_destination(), canonical);
        assert!(original.exists());
        assert!(list(&state).unwrap().is_empty());
        let created = PathBuf::from(create(&state).unwrap());
        assert_eq!(created.parent(), Some(canonical.as_path()));
        assert_eq!(list(&state).unwrap().len(), 1);
        assert_ne!(state.backup_dir(), canonical);
        let restarted = AppState::with_backups(state.backup_dir());
        assert_eq!(restarted.backup_destination(), canonical);
        let db = rbl_db::Library::open(location.clone(), rbl_db::OpenMode::ReadOnly).unwrap();
        let (library, _) = rbl_index::load(&db).unwrap();
        restarted.set_library(library, false, db.schema().db_version, 0, location);
        assert_eq!(list(&restarted).unwrap()[0].path, created.to_string_lossy());
        restore(&restarted, &original).unwrap();
        delete(&restarted, &created).unwrap();
        assert!(original.exists());
    }

    #[test]
    fn invalid_or_busy_destination_changes_keep_the_previous_setting() {
        let (dir, state, location) = fixture();
        let previous = state.backup_destination();
        let file = dir.path().join("not a folder");
        fs::write(&file, b"keep").unwrap();
        assert!(state.set_backup_destination(&file).is_err());
        assert!(state.set_backup_destination(&dir.path().join("missing")).is_err());
        let recursive = analysis(&location).join("backups");
        fs::create_dir_all(&recursive).unwrap();
        assert!(state.set_backup_destination(&recursive).is_err());
        assert_eq!(state.backup_destination(), previous);
        state.backup_progress.lock().running = true;
        assert!(state.set_backup_destination(dir.path()).is_err());
        assert_eq!(state.backup_destination(), previous);
        assert_eq!(AppState::with_backups(state.backup_dir()).backup_destination(), previous);
    }

    #[test]
    fn progress_reports_bytes_and_cancellation_discards_only_the_partial_copy() {
        let (_dir, state, location) = fixture();
        let file = analysis(&location).join("test/ANLZ0000.DAT");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, vec![1; 3 * 1024 * 1024]).unwrap();
        let mut last = (0, 0);
        let mut items = std::collections::HashSet::new();
        let saved = create_with_progress(&state, &mut |phase, copied, total, item| {
            items.insert(item.to_owned());
            if phase == "copying" {
                assert!(copied >= last.0);
                assert!(copied <= total);
                last = (copied, total);
            }
            Ok(())
        }).unwrap();
        assert!(last.0 > 3 * 1024 * 1024);
        assert_eq!(last.0, last.1);
        assert!(items.contains("Database · master.db"));
        assert!(items.contains("Analysis files · USBANLZ/test/ANLZ0000.DAT"));
        assert!(items.contains("Checking database · master.db"));
        // Keep a prior snapshot while testing cancellation of a new one.
        let previous = state.backup_destination().join("library-previous.zip");
        fs::rename(&saved, &previous).unwrap();
        let before = rating(&state);
        let result = create_with_progress(&state, &mut |phase, copied, _, _| {
            if phase == "copying" && copied > 0 {
                Err(AppError::new(crate::error::ErrorKind::Cancelled, "Backup stopped."))
            } else { Ok(()) }
        });
        assert_eq!(result.unwrap_err().kind, crate::error::ErrorKind::Cancelled);
        assert_eq!(rating(&state), before);
        assert_eq!(list(&state).unwrap().len(), 1);
        assert_eq!(PathBuf::from(&list(&state).unwrap()[0].path), previous.canonicalize().unwrap());
        assert!(fs::read_dir(state.backup_dir()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".partial-")));
        assert_eq!(fs::metadata(file).unwrap().len(), 3 * 1024 * 1024);
    }

    #[test]
    fn background_job_rejects_duplicates_and_can_be_stopped_while_waiting() {
        let (_dir, state, _location) = fixture();
        let state = std::sync::Arc::new(state);
        let gate = state.edit_gate.lock();
        start(state.clone()).unwrap();
        assert!(state.backup_progress.lock().running);
        assert!(start(state.clone()).is_err());
        cancel(&state);
        assert_eq!(state.backup_progress.lock().phase, "stopping");
        drop(gate);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.backup_progress.lock().running {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(state.backup_progress.lock().phase, "cancelled");
        assert!(list(&state).unwrap().is_empty());
    }

    fn rating(state: &AppState) -> u8 {
        state
            .read_db(|db| {
                Ok(db.connection().query_row(
                    "SELECT Rating FROM djmdContent WHERE ID=?1",
                    [rbl_db::fixture::track_id(1)],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }
    #[test]
    fn artwork_vocals_and_library_selections_round_trip() {
        let (_dir, state, location) = fixture();
        let art = artwork(&location).join("abc/cover/artwork_m.jpg");
        fs::create_dir_all(art.parent().unwrap()).unwrap();
        fs::write(&art, b"thumbnail bytes").unwrap();
        let vocals = analysis(&location).join("abc/ANLZ0000.2EX");
        fs::create_dir_all(vocals.parent().unwrap()).unwrap();
        let mut anlz = b"PMAI".to_vec();
        for word in [12u32, 40] { anlz.extend(word.to_be_bytes()); }
        anlz.extend(b"PVDI");
        for word in [24u32, 28, 1024, 0x56220001, 4] { anlz.extend(word.to_be_bytes()); }
        anlz.extend([0, 2, 4, 1]);
        fs::write(&vocals, &anlz).unwrap();
        let root = location.master_db.parent().unwrap();
        for name in LIBRARY_FILES { fs::write(root.join(name), b"original selections").unwrap(); }
        let mut items = Vec::new();
        let snapshot = create_with_progress(&state, &mut |_, _, _, item| { items.push(item.to_owned()); Ok(()) }).unwrap();
        assert!(list(&state).unwrap()[0].includes_artwork);
        assert!(items.iter().any(|s| s.contains("Artwork thumbnails · Artwork/abc/cover/artwork_m.jpg")));
        fs::write(&art, b"changed artwork").unwrap();
        fs::write(&vocals, b"changed vocals").unwrap();
        for name in LIBRARY_FILES { fs::write(root.join(name), b"changed selections").unwrap(); }
        restore(&state, Path::new(&snapshot)).unwrap();
        assert_eq!(fs::read(&art).unwrap(), b"thumbnail bytes");
        assert_eq!(fs::read(&vocals).unwrap(), anlz);
        assert_eq!(rbl_anlz::Anlz::read(&vocals).unwrap().vocals().unwrap(), &[0, 2, 4, 1]);
        for name in LIBRARY_FILES { assert_eq!(fs::read(root.join(name)).unwrap(), b"original selections"); }
    }

    #[test]
    fn missing_artwork_rejects_new_snapshots_but_legacy_snapshots_preserve_live_artwork() {
        let (_dir, state, location) = fixture();
        let zip = PathBuf::from(create(&state).unwrap());
        let unpacked = crate::backup_zip::extract(&zip, state.backup_dir()).unwrap();
        let snapshot = state.backup_dir().join("library-missing-artwork");
        fs::rename(&unpacked.0, &snapshot).unwrap();
        fs::remove_dir_all(snapshot.join("artwork")).unwrap();
        let art = artwork(&location).join("current.jpg");
        fs::create_dir_all(art.parent().unwrap()).unwrap();
        fs::write(&art, b"keep artwork").unwrap();
        let before = rating(&state);
        assert!(restore(&state, &snapshot).is_err());
        assert_eq!(rating(&state), before);
        let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(snapshot.join("manifest.json")).unwrap()).unwrap();
        saved.as_object_mut().unwrap().remove("includes_artwork");
        saved.as_object_mut().unwrap().remove("library_files");
        fs::write(snapshot.join("manifest.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
        restore(&state, &snapshot).unwrap();
        assert_eq!(fs::read(&art).unwrap(), b"keep artwork");
        assert!(!list(&state).unwrap().iter().find(|entry| entry.name == "library-missing-artwork").unwrap().includes_artwork);
    }

    #[test]
    fn backups_round_trip_database_analysis_and_wal_and_delete_only_the_snapshot() {
        let (_dir, state, location) = fixture();
        let track = rbl_db::fixture::track_id(1);
        // Keep a WAL connection alive to ensure committed WAL pages are included.
        let db = rbl_db::Library::open(location.clone(), rbl_db::OpenMode::ReadWrite).unwrap();
        db.connection()
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .unwrap();
        db.connection().execute("UPDATE djmdContent SET Rating=3 WHERE ID=?1", [&track]).unwrap();
        let file = analysis(&location).join("test/ANLZ0000.DAT");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"original grid").unwrap();
        let music = location.share_root.join("music.mp3");
        fs::write(&music, b"music stays here").unwrap();
        let path = PathBuf::from(create(&state).unwrap());
        drop(db);
        let entries = list(&state).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].bytes > 0 && entries[0].created_at > 0 && entries[0].includes_analysis);
        assert_eq!(entries[0].bytes, fs::metadata(&path).unwrap().len(), "saved size is the compressed ZIP size");
        assert!(!path.join("music.mp3").exists());
        state.write(|w| w.set_rating(&track, 5)).unwrap();
        assert_eq!(rating(&state), 5); // Prime the cached reader before restore.
        fs::write(&file, b"edited grid").unwrap();
        let extra = analysis(&location).join("later.DAT");
        fs::write(&extra, b"new analysis").unwrap();
        restore(&state, &path).unwrap();
        assert_eq!(rating(&state), 3);
        assert_eq!(fs::read(&file).unwrap(), b"original grid");
        assert!(!extra.exists());
        assert_eq!(fs::read(&music).unwrap(), b"music stays here");
        assert!(!journal(state.backup_dir()).exists());
        // Reopening the app still sees the backup, rather than a memory-only list.
        assert_eq!(list(&state).unwrap().len(), 1);
        delete(&state, &path).unwrap();
        assert!(list(&state).unwrap().is_empty());
        assert_eq!(rating(&state), 3);
        assert!(file.exists());
        assert!(delete(&state, &location.master_db).is_err());
    }
    #[test]
    fn legacy_database_backups_remain_restorable_without_replacing_analysis() {
        let (_dir, state, location) = fixture();
        let before = rating(&state);
        let backup = rbl_db::write::Writer::open(location.clone(), state.backup_dir().to_path_buf()).unwrap().back_up_now().unwrap();
        state
            .write(|w| w.set_rating(&rbl_db::fixture::track_id(1), (before + 1) % 6))
            .unwrap();
        let file = analysis(&location).join("current.DAT");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"current analysis").unwrap();
        let entries = list(&state).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].includes_analysis);
        restore(&state, &backup).unwrap();
        assert_eq!(rating(&state), before);
        assert_eq!(fs::read(&file).unwrap(), b"current analysis");
        delete(&state, &backup).unwrap();
        assert!(list(&state).unwrap().is_empty());
    }

    #[test]
    fn older_directory_snapshots_still_restore_and_delete() {
        let (_dir, state, location) = fixture();
        let before = rating(&state);
        let zip = PathBuf::from(create(&state).unwrap());
        let unpacked = crate::backup_zip::extract(&zip, state.backup_dir()).unwrap();
        let old = state.backup_dir().join("library-legacy-directory");
        fs::rename(&unpacked.0, &old).unwrap();
        let mut saved = manifest(&old, &location).unwrap();
        saved.version = 1;
        fs::write(old.join("manifest.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
        state.write(|w| w.set_rating(&rbl_db::fixture::track_id(1), (before + 1) % 6)).unwrap();
        restore(&state, &old).unwrap();
        assert_eq!(rating(&state), before);
        delete(&state, &old).unwrap();
        assert!(zip.exists());
    }

    #[test]
    fn corrupt_or_incomplete_backup_does_not_change_the_live_library() {
        let (_dir, state, location) = fixture();
        let path = PathBuf::from(create(&state).unwrap());
        let before = rating(&state);
        assert_eq!(path.extension().unwrap(), "zip");
        fs::write(&path, b"broken").unwrap();
        assert!(restore(&state, &path).is_err());
        assert_eq!(rating(&state), before);
        assert!(!journal(state.backup_dir()).exists());
        fs::write(&path, b"PK").unwrap();
        assert!(restore(&state, &path).is_err());
        assert!(location.master_db.exists());
    }
    #[test]
    fn interrupted_restore_rolls_back_and_committed_restore_only_cleans_up() {
        let (_dir, state, location) = fixture();
        fs::create_dir_all(state.backup_dir()).unwrap();
        let target = analysis(&location);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("grid"), b"before").unwrap();
        let parent = target.parent().unwrap();
        let staged = parent.join(".rbxport-restore-test-new");
        let previous = parent.join(".rbxport-restore-test-old");
        for committed in [false, true] {
            fs::create_dir(&staged).unwrap();
            fs::write(staged.join("grid"), b"after").unwrap();
            let intent = Restore {
                library: location.master_db.clone(),
                committed,
                swaps: vec![Swap {
                    target: target.clone(),
                    staged: staged.clone(),
                    previous: previous.clone(),
                    existed: true,
                }],
            };
            save_restore(state.backup_dir(), &intent).unwrap();
            assert!(state
                .write(|w| w.set_rating(&rbl_db::fixture::track_id(1), 5))
                .is_err());
            fs::rename(&target, &previous).unwrap();
            fs::rename(&staged, &target).unwrap();
            recover(state.backup_dir(), &location).unwrap();
            recover(state.backup_dir(), &location).unwrap();
            assert_eq!(
                fs::read(target.join("grid")).unwrap(),
                if committed {
                    b"after".as_slice()
                } else {
                    b"before".as_slice()
                }
            );
            assert!(!previous.exists());
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_backup_and_analysis_paths_are_refused() {
        let (_dir, state, location) = fixture();
        fs::create_dir_all(state.backup_dir()).unwrap();
        let alias = state.backup_dir().join("master-link.db");
        std::os::unix::fs::symlink(&location.master_db, &alias).unwrap();
        assert!(delete(&state, &alias).is_err());
        assert!(restore(&state, &alias).is_err());
        fs::create_dir_all(analysis(&location)).unwrap();
        std::os::unix::fs::symlink(&location.master_db, analysis(&location).join("escape"))
            .unwrap();
        assert!(create(&state).is_err());
        assert!(location.master_db.exists());
    }
}
