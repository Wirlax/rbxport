//! Standard ZIP snapshots: parallel per-file compression, then raw assembly.
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub fn compressed_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".zip");
    name.into()
}

pub fn compress_file(
    source: &Path,
    target: &Path,
    progress: &mut dyn FnMut(u64) -> io::Result<()>,
) -> io::Result<u64> {
    progress(0)?;
    let meta = fs::symlink_metadata(source)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(io::Error::other("Expected a regular backup file"));
    }
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(compressed_path(target))?;
    let mut zip = ZipWriter::new(output);
    zip.start_file(
        "data",
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(9))
            .large_file(meta.len() >= u64::from(u32::MAX)),
    )?;
    let mut source = fs::File::open(source)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut copied = 0;
    loop {
        progress(0)?;
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        zip.write_all(&buffer[..count])?;
        copied += count as u64;
        progress(count as u64)?;
    }
    zip.finish()?.sync_all()?;
    Ok(copied)
}

pub fn assemble(
    root: &Path,
    target: &Path,
    check: &mut dyn FnMut() -> io::Result<()>,
) -> io::Result<()> {
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut zip = ZipWriter::new(output);
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        check()?;
        for entry in fs::read_dir(&directory)? {
            check()?;
            let path = entry?.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                return Err(io::Error::other("Unexpected symbolic link"));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            if meta.is_dir() {
                zip.add_directory(format!("{relative}/"), SimpleFileOptions::default())?;
                pending.push(path);
            } else if relative == "manifest.json" {
                zip.start_file(
                    relative,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .compression_level(Some(9)),
                )?;
                zip.write_all(&fs::read(&path)?)?;
            } else {
                let name = relative
                    .strip_suffix(".zip")
                    .ok_or_else(|| io::Error::other("Uncompressed backup entry"))?;
                let mut entry = ZipArchive::new(fs::File::open(&path)?)?;
                zip.raw_copy_file_rename(entry.by_index(0)?, name)?;
            }
        }
    }
    zip.finish()?.sync_all()
}

pub fn manifest(path: &Path) -> io::Result<Vec<u8>> {
    let mut zip = ZipArchive::new(fs::File::open(path)?)?;
    let mut entry = zip.by_name("manifest.json")?;
    if entry.size() > 1024 * 1024 {
        return Err(io::Error::other("Invalid backup manifest"));
    }
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Temporary extraction is never the live library. CRC and database validation
/// must succeed before the existing restore journal can publish any files.
pub struct Extracted(pub PathBuf);
impl Drop for Extracted {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub fn extract(path: &Path, root: &Path) -> io::Result<Extracted> {
    let extracted = Extracted(root.join(format!(".restore-unpack-{}", uuid::Uuid::new_v4())));
    fs::create_dir(&extracted.0)?;
    let mut zip = ZipArchive::new(fs::File::open(path)?)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| io::Error::other("Unsafe backup entry"))?;
        let valid = name == Path::new("manifest.json")
            || name == Path::new("master.db")
            || name == Path::new("master.db-wal")
            || crate::backups::LIBRARY_FILES.iter().any(|file| name == Path::new(file))
            || name.starts_with("analysis")
            || name.starts_with("artwork");
        if !valid
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(io::Error::other("Unexpected backup entry"));
        }
        let target = extracted.0.join(name);
        if entry.is_dir() {
            fs::create_dir_all(target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            io::copy(&mut entry, &mut output)?;
            output.sync_all()?;
        }
    }
    Ok(extracted)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn compression_assembles_a_standard_zip_and_restores_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        fs::create_dir_all(stage.join("analysis/empty")).unwrap();
        let source = dir.path().join("source");
        let data = vec![7; 1024 * 1024];
        fs::write(&source, &data).unwrap();
        compress_file(&source, &stage.join("analysis/ANLZ.DAT"), &mut |_| Ok(())).unwrap();
        fs::write(stage.join("manifest.json"), b"{}").unwrap();
        let archive = dir.path().join("backup.zip");
        assemble(&stage, &archive, &mut || Ok(())).unwrap();
        assert!(fs::metadata(&archive).unwrap().len() < 10_000);
        let restored = extract(&archive, dir.path()).unwrap();
        assert_eq!(
            fs::read(restored.0.join("analysis/ANLZ.DAT")).unwrap(),
            data
        );
        assert!(restored.0.join("analysis/empty").is_dir());
        let path = restored.0.clone();
        drop(restored);
        assert!(!path.exists());
    }
    #[test]
    fn unsafe_archive_entries_are_rejected_and_extraction_is_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.zip");
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("../escape", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(extract(&path, dir.path()).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn damaged_file_crc_rejects_extraction() {
        use std::io::{Seek, SeekFrom};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.zip");
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file(
            "master.db",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(b"database contents").unwrap();
        zip.finish().unwrap();
        let mut archive = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let start = archive.by_index(0).unwrap().data_start();
        drop(archive);
        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(start)).unwrap();
        file.write_all(b"X").unwrap();
        drop(file);
        assert!(extract(&path, dir.path()).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn compression_can_be_cancelled_between_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::write(&source, vec![8; 3 * 1024 * 1024]).unwrap();
        let result = compress_file(&source, &dir.path().join("target"), &mut |bytes| {
            if bytes > 0 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::metadata(source).unwrap().len(), 3 * 1024 * 1024);
    }
}
