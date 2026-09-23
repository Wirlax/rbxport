//! Durable replacement through a unique sibling, with data flushed before
//! rename and the containing directory flushed afterwards on Unix.
use std::io::Write;
use std::path::Path;

pub fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = path.as_ref();
    let mut missing = Vec::new();
    let mut at = path;
    while !at.exists() {
        missing.push(at.to_path_buf());
        let Some(parent) = at.parent().filter(|p| !p.as_os_str().is_empty()) else {
            break;
        };
        at = parent;
    }
    std::fs::create_dir_all(path)?;
    for dir in missing.iter().rev() {
        sync_dir(
            dir.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?;
    }
    Ok(())
}

pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    sync_dir(parent)
}

/// Flush a complete staged file before atomically replacing the destination.
/// Both paths must be on the same filesystem, and database handles closed.
pub fn replace(staged: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(staged)?
        .sync_all()?;
    std::fs::rename(staged, target)?;
    sync_dir(
        target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
}

/// Copy without truncating the destination, then durably publish it.
pub fn copy(source: &Path, target: &Path) -> std::io::Result<u64> {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = std::io::copy(&mut std::fs::File::open(source)?, &mut temp)?;
    temp.as_file().sync_all()?;
    temp.persist(target).map_err(|e| e.error)?;
    sync_dir(parent)?;
    Ok(bytes)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PublicationEntry {
    path: std::path::PathBuf,
    present: bool,
}

/// Durable commit intent for a set of files. Recovery rolls publication forward
/// using retained images, so recovery itself can be interrupted repeatedly.
/// Callers must serialize access and recover before exposing the file set.
pub struct Publication {
    root: std::path::PathBuf,
    journal: std::path::PathBuf,
    stage: tempfile::TempDir,
    _lock: std::fs::File,
    #[cfg(unix)]
    root_handle: std::fs::File,
}

impl Publication {
    pub fn new(root: &Path, name: &str) -> std::io::Result<Self> {
        create_dir_all(root)?;
        let lock = lock(root)?;
        let journal = root.join(name);
        if journal.try_exists()? {
            Self::finish(root, &journal)?;
        }
        Ok(Self {
            root: root.to_owned(),
            journal,
            stage: tempfile::tempdir_in(root)?,
            _lock: lock,
            #[cfg(unix)]
            root_handle: std::fs::File::open(root)?,
        })
    }

    pub fn check_root(&self) -> std::io::Result<()> {
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            let held=self.root_handle.metadata()?;
            let current=std::fs::metadata(&self.root)?;
            if held.dev()!=current.dev() || held.ino()!=current.ino() {
                return Err(std::io::Error::other("The destination volume changed during publication"));
            }
        }
        Ok(())
    }

    pub fn stage(&self) -> &Path {
        self.stage.path()
    }

    /// Missing staged files mean deletion. Paths are relative to root.
    pub fn commit(&self, paths: &[std::path::PathBuf]) -> std::io::Result<()> {
        self.check_root()?;
        validate_paths(paths)?;
        let mut entries = Vec::with_capacity(paths.len());
        let mut directories = std::collections::BTreeSet::new();
        for path in paths {
            let image = self.stage.path().join(path);
            let present = image.try_exists()?;
            if present {
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&image)?
                    .sync_all()?;
            }
            entries.push(PublicationEntry {
                path: path.clone(),
                present,
            });
            let mut parent = image.parent();
            while let Some(dir) = parent.filter(|p| p.starts_with(self.stage.path())) {
                if dir.try_exists()? {
                    directories.insert(dir.to_path_buf());
                }
                parent = dir.parent();
            }
        }
        // Flush each directory once, children before parents.
        for directory in directories.iter().rev() {
            sync_dir(directory)?;
        }
        write(
            &self.stage.path().join("publication.json"),
            &serde_json::to_vec(&entries)?,
        )?;
        std::fs::rename(self.stage.path(), &self.journal)?;
        sync_dir(&self.root)?;
        Self::finish(&self.root, &self.journal)
    }

    pub fn recover(root: &Path, name: &str) -> std::io::Result<()> {
        let journal = root.join(name);
        if journal.try_exists()? {
            let _lock = lock(root)?;
            if journal.try_exists()? {
                Self::finish(root, &journal)?;
            }
        }
        Ok(())
    }

    fn finish(root: &Path, journal: &Path) -> std::io::Result<()> {
        let entries: Vec<PublicationEntry> =
            serde_json::from_slice(&std::fs::read(journal.join("publication.json"))?)?;
        validate_paths(
            &entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
        )?;
        #[cfg(unix)]
        let root_handle = std::fs::File::open(root)?;
        // The previous generation stays inside the journal until every new
        // image is in place. All moves are within one filesystem, so a large
        // audio file is published with atomic renames instead of being copied
        // over the USB a second time. Every intermediate state is recognizable
        // and recovery always rolls forward.
        for entry in entries {
            #[cfg(unix)] {
                use std::os::unix::fs::MetadataExt;
                let held=root_handle.metadata()?; let current=std::fs::metadata(root)?;
                if held.dev()!=current.dev() || held.ino()!=current.ino() { return Err(std::io::Error::other("Destination volume changed during publication")); }
            }
            let image = journal.join(&entry.path);
            let target = root.join(&entry.path);
            let previous = journal.join(".previous").join(&entry.path);
            if entry.present {
                if image.try_exists()? {
                    if target.try_exists()? {
                        if target.is_dir() {
                            return Err(std::io::Error::other(format!(
                                "Publication target is a directory: {}",
                                entry.path.display()
                            )));
                        }
                        if previous.try_exists()? {
                            return Err(std::io::Error::other(format!(
                                "Publication conflict at {}; recovery data was retained",
                                entry.path.display()
                            )));
                        }
                        if let Some(parent) = previous.parent() { create_dir_all(parent)?; }
                        std::fs::rename(&target, &previous)?;
                        sync_dir(target.parent().unwrap_or(root))?;
                    }
                    if let Some(parent) = target.parent() { create_dir_all(parent)?; }
                    std::fs::rename(&image, &target)?;
                    sync_dir(target.parent().unwrap_or(root))?;
                } else if !target.try_exists()? {
                    // A lost new image must never silently become a deletion.
                    // Put the old generation back when possible, then stop.
                    if previous.try_exists()? {
                        if let Some(parent) = target.parent() { create_dir_all(parent)?; }
                        std::fs::rename(&previous, &target)?;
                        sync_dir(target.parent().unwrap_or(root))?;
                    }
                    return Err(std::io::Error::other(format!(
                        "Missing publication image {}; the previous file was restored",
                        entry.path.display()
                    )));
                }
            } else if target.try_exists()? && !previous.try_exists()? {
                if let Some(parent) = previous.parent() { create_dir_all(parent)?; }
                std::fs::rename(&target, &previous)?;
                sync_dir(target.parent().unwrap_or(root))?;
            }
        }
        // Remove the commit intent atomically BEFORE deleting its images.
        // A crash during cleanup must never replay a missing image as deletion.
        let discarded = tempfile::tempdir_in(root)?;
        std::fs::rename(journal, discarded.path().join("completed"))?;
        if let Err(error) = sync_dir(root) {
            // Until retirement is durable, a crash may resurrect the journal.
            // Keep its images instead of deleting them during TempDir::drop.
            let _retained = discarded.keep();
            return Err(error);
        }
        Ok(())
    }
}

fn validate_paths(paths: &[std::path::PathBuf]) -> std::io::Result<()> {
    if paths.iter().any(|p| {
        p.as_os_str().is_empty()
            || p.components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
    }) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "publication paths must be relative",
        ));
    }
    Ok(())
}

/// Hold after recovery while inspecting/importing a device. Export publishers
/// use the matching exclusive lock for the whole operation.
pub fn read_lock(root: &Path) -> std::io::Result<std::fs::File> {
    let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join(".rbxport-write.lock"))?;
    fs2::FileExt::lock_shared(&file)?;
    Ok(file)
}

fn lock(root: &Path) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".rbxport-write.lock"))?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn failed_publication_replays_retained_images_and_deletions() {
        let root = tempfile::tempdir().unwrap();
        write(&root.path().join("first"), b"old").unwrap();
        write(&root.path().join("obsolete"), b"old").unwrap();
        // Force failure after publishing the first file.
        std::fs::create_dir(root.path().join("second")).unwrap();
        let publication = Publication::new(root.path(), ".journal").unwrap();
        write(&publication.stage().join("first"), b"new first").unwrap();
        write(&publication.stage().join("second"), b"new second").unwrap();
        assert!(publication
            .commit(&["first".into(), "second".into(), "obsolete".into()])
            .is_err());
        drop(publication);
        assert_eq!(
            std::fs::read(root.path().join("first")).unwrap(),
            b"new first"
        );
        assert!(Publication::recover(root.path(), ".journal").is_err());
        assert!(root.path().join(".journal/.previous/first").is_file());
        std::fs::remove_dir(root.path().join("second")).unwrap();
        Publication::recover(root.path(), ".journal").unwrap();
        Publication::recover(root.path(), ".journal").unwrap();
        assert_eq!(
            std::fs::read(root.path().join("second")).unwrap(),
            b"new second"
        );
        assert!(!root.path().join("obsolete").exists());
        assert!(!root.path().join(".journal").exists());
    }
    #[test]
    fn abandoned_staging_never_modifies_published_files() {
        let root = tempfile::tempdir().unwrap();
        write(&root.path().join("db"), b"old").unwrap();
        let publication = Publication::new(root.path(), ".journal").unwrap();
        write(&publication.stage().join("db"), b"new").unwrap();
        drop(publication);
        Publication::recover(root.path(), ".journal").unwrap();
        assert_eq!(std::fs::read(root.path().join("db")).unwrap(), b"old");
    }
}
