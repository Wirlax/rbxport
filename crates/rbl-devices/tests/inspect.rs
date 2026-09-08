//! Reading a real export back off a volume.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_export::{export, SourcePlaylist, SourceTrack};

fn track(dir: &std::path::Path, id: u64, title: &str) -> SourceTrack {
    let path = dir.join(format!("source-{id}.mp3"));
    std::fs::write(&path, vec![id as u8; 1024]).unwrap();
    SourceTrack {
        id,
        source_path: path,
        title: title.into(),
        artist: "TRIODE".into(),
        album: "Single".into(),
        duration_sec: 300,
        analysis: vec![("DAT".into(), rbl_anlz::AnlzBuilder::new().path("/x.mp3").finish())],
        ..SourceTrack::default()
    }
}

#[test]
fn a_stick_we_wrote_reports_what_is_on_it_and_that_it_can_be_synced() {
    let src = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "One"), track(src.path(), 2, "Two")];
    let playlists = vec![SourcePlaylist { name: "Set".into(), track_indices: vec![0, 1] }];
    export(stick.path(), &tracks, &playlists).unwrap();

    let found = rbl_devices::inspect(stick.path()).expect("an export");
    assert_eq!(found.tracks, 2);
    assert_eq!(found.playlists, 1);
    assert!(found.ours, "we wrote it, so the next export can be a sync");
    assert!(!found.written.is_empty(), "and it says when");
}

#[test]
fn a_stick_someone_else_wrote_still_reports_its_contents() {
    let src = tempfile::tempdir().unwrap();
    let stick = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "One")];
    let playlists = vec![SourcePlaylist { name: "Set".into(), track_indices: vec![0] }];
    export(stick.path(), &tracks, &playlists).unwrap();

    // As rekordbox would have left it: the databases, but no record of ours.
    std::fs::remove_file(rbl_export::Manifest::path(stick.path())).unwrap();

    let found = rbl_devices::inspect(stick.path()).expect("an export");
    assert_eq!(found.tracks, 1, "read out of export.pdb instead");
    assert_eq!(found.playlists, 1);
    assert!(!found.ours, "which means the next export writes it in full");
    assert!(found.written.is_empty());
}

#[test]
fn a_stick_with_only_a_manifest_is_not_an_export() {
    let stick = tempfile::tempdir().unwrap();
    let manifest = rbl_export::Manifest {
        version: rbl_export::manifest::MANIFEST_VERSION,
        written: "2026-09-08 00:00:00.000 +00:00".to_owned(),
        tracks: Vec::new(),
    };
    manifest.save(stick.path()).unwrap();
    // Without export.pdb no player can read it, so there is nothing to report.
    assert_eq!(rbl_devices::inspect(stick.path()), None);
}
