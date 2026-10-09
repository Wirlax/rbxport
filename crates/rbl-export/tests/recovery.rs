//! Syncs onto a stick in states a USB export meets in use.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, SystemTime};

use rbl_export::{export_full, verify, SourcePlaylist, SourceTrack, SyncNode, SyncSource};

fn tracks(src: &Path) -> Vec<SourceTrack> {
    (0..3_u64)
        .map(|i| {
            let path = src.join(format!("Kesä {i}.mp3"));
            std::fs::write(&path, vec![i as u8 + 1; 4096]).unwrap();
            SourceTrack {
                id: i + 1,
                source_path: path,
                title: format!("Track {i}"),
                artist: "Björk".into(),
                album: "Album".into(),
                analysis: vec![("DAT".into(), rbl_anlz::AnlzBuilder::new().path("/x.mp3").finish())],
                ..Default::default()
            }
        })
        .collect()
}

fn sync(root: &Path, tracks: &[SourceTrack]) -> rbl_export::Result<rbl_export::ExportReport> {
    let playlists = vec![SourcePlaylist { id: 10, name: "Set".into(), track_indices: (0..tracks.len()).collect(), ..Default::default() }];
    let source = SyncSource { db_id: 123, tree: vec![SyncNode { id: 10, parent: 0, attribute: 0 }], automatic: false };
    export_full(root, tracks, &playlists, &[], None, Some(&source), &mut |_| {})
}

/// FAT keeps local time with no zone, so a stick last written somewhere
/// ahead of this machine's clock carries files that look newer than now.
/// A sync's own publication took that for another writer and failed at 99%
/// with "the device changed after sync failed".
#[test]
fn a_stick_whose_files_look_newer_than_this_clock_still_syncs() {
    let stick = tempfile::tempdir().unwrap();
    let root = stick.path();
    let src = tempfile::tempdir().unwrap();
    let tracks = tracks(src.path());
    sync(root, &tracks).unwrap();
    let ahead = std::fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(3600));
    for file in ["PIONEER/rekordbox/exportLibrary.db", "PIONEER/rekordbox/export.pdb", "PIONEER/rbxport/manifest.json"] {
        std::fs::File::options().write(true).open(root.join(file)).unwrap().set_times(ahead).unwrap();
    }

    let again = sync(root, &tracks[1..]).unwrap();
    assert_eq!((again.tracks, again.removed), (2, 1));
    assert!(verify(root).unwrap().is_ok());
}
