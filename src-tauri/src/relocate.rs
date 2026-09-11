//! Auto Relocate: pointing missing tracks at files of the same name found
//! under the search folders from the Preferences window.
//!
//! The search is by file name alone, as rekordbox's own is described: a
//! track whose `FileNameL` turns up under a search folder is pointed at the
//! first one found, folders searched in the order given. Nothing else about
//! the file is checked — a same-named file that is a different recording is
//! the user's to notice, and the manual Locate button is there to fix it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::commands::{blocking, reload, write_error};
use crate::error::AppResult;
use crate::state::AppState;

/// What an automatic relocate did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateReportDto {
    pub relocated: u32,
    /// Missing tracks whose file name was found in none of the folders.
    pub unresolved: u32,
}

/// How deep under a search folder the walk goes. A music library is a
/// handful of levels; a folder that is a whole drive is not walked to the
/// bottom.
const MAX_DEPTH: usize = 16;
/// How many entries are looked at in all, so a search folder pointed at the
/// root of a disk ends rather than runs for minutes.
const MAX_ENTRIES: usize = 500_000;

/// Every file under `folders`, by name, the first found winning.
///
/// Walked breadth-first per folder in the order given, so a name that
/// appears twice resolves to the shallower one in the earlier folder — the
/// one somebody would point at by hand.
pub fn index_folders(folders: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    let mut seen = 0_usize;
    for folder in folders {
        let mut level: Vec<PathBuf> = vec![folder.clone()];
        for _ in 0..MAX_DEPTH {
            let mut next = Vec::new();
            for dir in &level {
                // perf-ok: a plain function, run under `blocking` by the command below.
                let Ok(entries) = std::fs::read_dir(dir) else { continue };
                for entry in entries.flatten() {
                    seen += 1;
                    if seen > MAX_ENTRIES {
                        return found;
                    }
                    let path = entry.path();
                    let Ok(kind) = entry.file_type() else { continue };
                    if kind.is_dir() {
                        // Hidden directories are skipped: `.Trashes`, `.Spotlight-V100`
                        // and the like hold copies nobody wants pointed at.
                        if entry.file_name().to_string_lossy().starts_with('.') {
                            continue;
                        }
                        next.push(path);
                    } else if kind.is_file() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        found.entry(name).or_insert(path);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            level = next;
        }
    }
    found
}

/// The file name a library row's path ends in.
fn file_name(path: &str) -> Option<String> {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Points every missing track at a same-named file under the folders.
#[tauri::command]
pub async fn auto_relocate<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    folders: Vec<String>,
) -> AppResult<RelocateReportDto> {
    let library = state.library()?;
    let state_for_reload = Arc::clone(&state);
    let writing = Arc::clone(&state);
    // `blocking` is `spawn_blocking` with a name: the walk and the writes
    // happen off the async thread.
    let report = blocking("auto_relocate", move || {
        // The missing tracks first, from the index: the walk is the slow
        // part, and a library with nothing missing need not walk at all.
        let mut missing: Vec<(String, String)> = Vec::new();
        for index in 0..library.len() {
            let path = library.folder_path.get(index);
            if path.is_empty() || Path::new(path).exists() {
                continue;
            }
            if let Some(name) = file_name(path) {
                missing.push((library.ids.get(index).copied().unwrap_or(0).to_string(), name));
            }
        }
        if missing.is_empty() {
            return Ok(RelocateReportDto { relocated: 0, unresolved: 0 });
        }

        let roots: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
        let found = index_folders(&roots);

        let mut writer = writing.open_writer().map_err(write_error)?;
        let mut relocated = 0_u32;
        let mut unresolved = 0_u32;
        for (id, name) in &missing {
            match found.get(name) {
                Some(path) => {
                    writer.relocate(id, path).map_err(write_error)?;
                    relocated += 1;
                }
                None => unresolved += 1,
            }
        }
        Ok(RelocateReportDto { relocated, unresolved })
    })
    .await?;

    // Only reload if anything moved; a search that found nothing has not
    // changed the library.
    if report.relocated > 0 {
        reload(app, state_for_reload).await?;
    }
    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_first_folder_and_the_shallower_file_win() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        // perf-ok: a test's fixture, not a command.
        std::fs::create_dir_all(a.join("deep")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("deep/song.mp3"), b"x").unwrap();
        std::fs::write(a.join("other.mp3"), b"x").unwrap();
        std::fs::write(b.join("song.mp3"), b"y").unwrap();

        let found = index_folders(&[a.clone(), b.clone()]);
        assert_eq!(found.get("song.mp3"), Some(&a.join("deep/song.mp3")));
        assert_eq!(found.get("other.mp3"), Some(&a.join("other.mp3")));

        let found = index_folders(&[b.clone(), a.clone()]);
        assert_eq!(found.get("song.mp3"), Some(&b.join("song.mp3")));
    }

    #[test]
    fn hidden_directories_and_missing_folders_are_passed_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".Trashes")).unwrap();
        std::fs::write(dir.path().join(".Trashes/song.mp3"), b"x").unwrap();
        let found = index_folders(&[dir.path().to_path_buf(), dir.path().join("nowhere")]);
        assert!(found.is_empty());
    }

    #[test]
    fn a_file_name_is_the_last_segment_of_a_row_path() {
        assert_eq!(file_name("/Users/x/Music/Track.aiff").as_deref(), Some("Track.aiff"));
        assert_eq!(file_name(""), None);
    }
}
