//! Logical bytes in the next library backup. Read only ANLZ section headers,
//! seeking over waveform payloads rather than loading every analysis file.
use crate::{
    error::{AppError, AppResult},
    state::AppState,
};
use serde::Serialize;
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSizes {
    pub updated_at: u64,
    pub database: u64,
    pub waveforms: u64,
    pub cues: u64,
    pub beat_grids: u64,
    pub phrases: u64,
    pub other: u64,
}
impl BackupSizes {
    fn add(&mut self, other: Self) {
        self.database += other.database;
        self.waveforms += other.waveforms;
        self.cues += other.cues;
        self.beat_grids += other.beat_grids;
        self.phrases += other.phrases;
        self.other += other.other;
    }
}

/// Owned by AppState, so all Preferences windows share one lazy scan per launch.
#[derive(Default)]
pub struct SizeCache {
    value: Option<AppResult<BackupSizes>>,
}
impl SizeCache {
    fn get(
        &mut self,
        refresh: bool,
        scan: impl FnOnce() -> AppResult<BackupSizes>,
    ) -> AppResult<BackupSizes> {
        if !refresh {
            if let Some(value) = &self.value {
                return value.clone();
            }
        }
        let result = scan();
        // Keep a previous successful reading if a manual refresh fails.
        // Cache an initial failure too; retrying must be an explicit request.
        if result.is_ok() || self.value.is_none() {
            self.value = Some(result.clone());
        }
        result
    }
}

pub fn cached(state: &AppState, refresh: bool) -> AppResult<BackupSizes> {
    // Hold only the scan cache lock. A second window joins the in-flight read
    // without blocking playback, editing, backups, or their progress queries.
    state.backup_sizes.lock().get(refresh, || measure(state))
}

fn measure(state: &AppState) -> AppResult<BackupSizes> {
    let location = state.location()?;
    measure_paths(
        &location.master_db,
        &location.share_root.join("PIONEER/USBANLZ"),
    )
    .map(|mut sizes| {
        sizes.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        sizes
    })
    .map_err(|e| AppError::internal(format!("Could not measure backup contents: {e}")))
}

fn regular_size(path: &Path) -> io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected a regular backup file",
        ));
    }
    Ok(meta.len())
}
fn measure_paths(database: &Path, analysis: &Path) -> io::Result<BackupSizes> {
    let mut sizes = BackupSizes {
        database: regular_size(database)?,
        ..Default::default()
    };
    let mut wal = database.as_os_str().to_os_string();
    wal.push("-wal");
    match regular_size(Path::new(&wal)) {
        Ok(bytes) => sizes.database += bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    match walk(analysis, &mut sizes) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound && !analysis.exists() => {}
        Err(e) => return Err(e),
    }
    Ok(sizes)
}
fn walk(path: &Path, sizes: &mut BackupSizes) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Symbolic links are not supported in backups",
        ));
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            walk(&entry?.path(), sizes)?;
        }
    } else if meta.is_file() {
        sizes.add(analysis_sizes(path, meta.len())?);
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported backup file",
        ));
    }
    Ok(())
}

fn analysis_sizes(path: &Path, length: u64) -> io::Result<BackupSizes> {
    let unknown = || BackupSizes {
        other: length,
        ..Default::default()
    };
    if length < 12 {
        return Ok(unknown());
    }
    let mut file = fs::File::open(path)?;
    let mut header = [0; 12];
    file.read_exact(&mut header)?;
    let number =
        |bytes: &[u8]| u64::from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
    let mut at = number(&header[4..8]);
    if &header[..4] != b"PMAI" || at < 12 || at > length {
        return Ok(unknown());
    }
    let mut sizes = BackupSizes {
        other: at,
        ..Default::default()
    };
    while length - at >= 12 {
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(&mut header)?;
        let header_len = number(&header[4..8]);
        let section_len = number(&header[8..12]);
        if header_len < 12 || section_len < header_len || section_len > length - at {
            return Ok(unknown());
        }
        match &header[..4] {
            b"PWAV" | b"PWV2" | b"PWV3" | b"PWV4" | b"PWV5" | b"PWV6" | b"PWV7" => {
                sizes.waveforms += section_len
            }
            b"PCOB" | b"PCO2" => sizes.cues += section_len,
            b"PQTZ" | b"PQT2" => sizes.beat_grids += section_len,
            b"PSSI" => sizes.phrases += section_len,
            _ => sizes.other += section_len,
        }
        at += section_len;
    }
    sizes.other += length - at;
    Ok(sizes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn scans_once_per_session_until_explicit_refresh() {
        let scans = std::cell::Cell::new(0);
        let scan = || {
            scans.set(scans.get() + 1);
            Ok(BackupSizes {
                updated_at: scans.get(),
                ..Default::default()
            })
        };
        let mut cache = SizeCache::default();
        assert_eq!(scans.get(), 0);
        assert_eq!(cache.get(false, scan).unwrap().updated_at, 1);
        assert_eq!(cache.get(false, scan).unwrap().updated_at, 1);
        assert_eq!(scans.get(), 1);
        assert_eq!(cache.get(true, scan).unwrap().updated_at, 2);
        assert_eq!(cache.get(false, scan).unwrap().updated_at, 2);
        let mut next_session = SizeCache::default();
        assert_eq!(next_session.get(false, scan).unwrap().updated_at, 3);
    }

    #[test]
    fn failed_scans_require_refresh_and_preserve_previous_success() {
        let mut cache = SizeCache::default();
        assert!(cache
            .get(false, || Err(AppError::internal("unavailable")))
            .is_err());
        assert!(cache.get(false, || Ok(BackupSizes::default())).is_err());
        assert!(cache
            .get(true, || Ok(BackupSizes {
                updated_at: 42,
                ..Default::default()
            }))
            .is_ok());
        assert!(cache
            .get(true, || Err(AppError::internal("unavailable")))
            .is_err());
        assert_eq!(
            cache
                .get(false, || Ok(BackupSizes::default()))
                .unwrap()
                .updated_at,
            42
        );
    }

    #[test]
    fn accounts_for_every_byte_in_database_wal_and_analysis_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("master.db");
        fs::write(&db, [0; 100]).unwrap();
        fs::write(dir.path().join("master.db-wal"), [0; 20]).unwrap();
        fs::write(dir.path().join("music.mp3"), [0; 99]).unwrap();
        let anlz = dir.path().join("analysis");
        fs::create_dir(&anlz).unwrap();
        let mut bytes = b"PMAI".to_vec();
        bytes.extend(12u32.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        for tag in [b"PWV7", b"PCOB", b"PQTZ", b"PSSI", b"PPTH"] {
            bytes.extend(tag);
            bytes.extend(12u32.to_be_bytes());
            bytes.extend(16u32.to_be_bytes());
            bytes.extend([0; 4]);
        }
        fs::write(anlz.join("ANLZ.2EX"), &bytes).unwrap();
        fs::write(anlz.join("unknown"), [0; 7]).unwrap();
        let sizes = measure_paths(&db, &anlz).unwrap();
        assert_eq!(sizes.database, 120);
        assert_eq!(
            (sizes.waveforms, sizes.cues, sizes.beat_grids, sizes.phrases),
            (16, 16, 16, 16)
        );
        assert_eq!(sizes.other, 12 + 16 + 7);
        assert_eq!(
            sizes.waveforms + sizes.cues + sizes.beat_grids + sizes.phrases + sizes.other,
            bytes.len() as u64 + 7
        );
    }
    #[test]
    fn malformed_analysis_is_counted_as_other_and_missing_analysis_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("master.db");
        fs::write(&db, [0; 8]).unwrap();
        assert_eq!(
            measure_paths(&db, &dir.path().join("absent"))
                .unwrap()
                .database,
            8
        );
        let file = dir.path().join("bad.dat");
        let bytes = b"PMAI\x00\x00\x00\x0c\x00\x00\x00\x18PWAV\x00\x00\x00\x0c\xff\xff\xff\xff";
        fs::write(&file, bytes).unwrap();
        let sizes = analysis_sizes(&file, bytes.len() as u64).unwrap();
        assert_eq!(sizes.other, bytes.len() as u64);
        assert_eq!(sizes.waveforms, 0);
    }
}
