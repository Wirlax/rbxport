//! The file tree a player reads audio from.
//!
//! rekordbox exports the host's whole filesystem root as `/` and a player
//! then `LOOKUP`s and `GETATTR`s every entry of every directory it lists
//! (8,868 calls for one track in the capture), so the tree served here holds
//! only the directories on the way to indexed files. The path a player asks
//! for is the absolute host path the track-info reply gave it, so the tree
//! must be rooted where those paths are.
//!
//! On macOS that is one export, `/`. On Windows rekordbox's paths are
//! `C:/Users/…` and the player mounts `/C/` — `[ASSUME]` from
//! alphatheta-connect's client, which resolves a Windows path that way; not
//! yet measured against a capture on Windows.

use rbl_index::Library;
use rbl_nfs::{Exports, Vfs};

/// The exports for a library: every indexed track under its export root.
pub fn exports(library: &Library) -> Exports {
    let mut exports = Exports::new();
    let mut trees: Vec<Vfs> = Vec::new();
    for row in 0..library.len() {
        let path = library.folder_path.get(row);
        let Some((export, relative)) = split(path) else { continue };
        let tree = trees.iter().position(|t| t.export_name() == export).unwrap_or_else(|| {
            trees.push(Vfs::new(export));
            trees.len() - 1
        });
        if let Some(tree) = trees.get_mut(tree) {
            // Sizes are read when a player first asks, not for 38,681 files
            // at start.
            tree.add_file_unsized(&relative, path);
        }
    }
    for tree in trees {
        exports.insert(tree);
    }
    exports
}

/// The export a path belongs to, and the path within it: rekordbox's
/// convention, as `rbl-nfs` states it.
pub fn split(path: &str) -> Option<(String, String)> {
    rbl_nfs::split_export(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use rbl_index::testing::{library_from, TestTrack};

    #[test]
    fn paths_split_into_an_export_and_a_relative_path() {
        assert_eq!(split("/Users/me/Music/a.mp3"), Some(("/".into(), "Users/me/Music/a.mp3".into())));
        assert_eq!(split("C:/Users/me/a.mp3"), Some(("/C/".into(), "Users/me/a.mp3".into())));
        assert_eq!(split(r"d:\Music\b.flac"), Some(("/D/".into(), "Music/b.flac".into())));
        assert_eq!(split("relative/path.mp3"), None);
        assert_eq!(split(""), None);
    }

    #[test]
    fn the_tree_holds_only_the_directories_on_the_way_to_tracks() {
        let track = |id, path| TestTrack { id, title: "t", path, ..TestTrack::default() };
        let library = library_from(&[
            track(1, "/Volumes/SD/RB/A/one.mp3"),
            track(2, "/Volumes/SD/RB/A/two.mp3"),
            track(3, "/Users/me/three.mp3"),
            track(4, "not absolute.mp3"),
        ]);
        let exports = exports(&library);
        let root = exports.get("/").unwrap();
        let top = root.children(root.root()).iter().map(|&i| root.name(i).unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(top, ["Volumes", "Users"]);
        let a = root.resolve("Volumes/SD/RB/A").unwrap();
        assert_eq!(root.children(a).len(), 2);
        assert_eq!(root.source(root.resolve("Users/me/three.mp3").unwrap()).unwrap().to_str(), Some("/Users/me/three.mp3"));
        assert_eq!(exports.names().len(), 1);
    }
}
