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

pub fn create_dir_all(path: &Path) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut at = path;
    while !at.exists() {
        missing.push(at.to_path_buf());
        let Some(parent) = at.parent().filter(|p| !p.as_os_str().is_empty()) else { break };
        at = parent;
    }
    std::fs::create_dir_all(path)?;
    for dir in missing.iter().rev() {
        sync_dir(dir.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")))?;
    }
    Ok(())
}

pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let temp = parent.join(format!(".rbxport-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        sync_dir(parent)
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temp); }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn replaces_existing_files_and_cleans_up_on_rename_failure() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("test.dat");
        super::write(&file, b"first").unwrap();
        super::write(&file, b"second").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"second");
        assert!(super::write(root.path(), b"cannot replace directory").is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
