//! End-to-end: write an export to a temp directory, then read it back the way
//! a player would.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use rbl_export::{export, verify, SourcePlaylist, SourceTrack};

/// Writes a dummy audio file and returns a track that points at it.
fn track(dir: &std::path::Path, n: u32, title: &str, artist: &str) -> SourceTrack {
    let path = dir.join(format!("source-{n}.mp3"));
    std::fs::write(&path, vec![n as u8; 2048]).unwrap();
    SourceTrack {
        source_path: path,
        title: title.into(),
        artist: artist.into(),
        album: "Single".into(),
        genre: "House".into(),
        key: "Am".into(),
        bpm_x100: 12_800,
        duration_sec: 300,
        rating: 4,
        date_added: "2026-09-06".into(),
        analysis: vec![("DAT".into(), rbl_anlz::AnlzBuilder::new().path("/x.mp3").finish())],
        ..SourceTrack::default()
    }
}

#[test]
fn writes_a_tree_a_player_can_browse() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(src.path(), 1, "All U Need", "TRIODE"),
        track(src.path(), 2, "The Abyss", "ARTBAT"),
    ];
    let playlists = vec![SourcePlaylist { name: "Melodic Vox".into(), track_indices: vec![0, 1] }];

    let report = export(dest.path(), &tracks, &playlists).unwrap();
    assert_eq!(report.tracks, 2);
    assert_eq!(report.playlists, 1);
    assert_eq!(report.analysis_files, 2);
    assert!(report.skipped.is_empty());

    // The layout a CDJ expects.
    assert!(dest.path().join("PIONEER/rekordbox/export.pdb").is_file());
    assert!(dest.path().join("Contents/TRIODE/Single/source-1.mp3").is_file());
    assert!(dest.path().join("Contents/ARTBAT/Single/source-2.mp3").is_file());

    let check = verify(dest.path()).unwrap();
    assert!(check.parsed);
    assert_eq!(check.tracks, 2);
    assert_eq!(check.playlists, 1);
    assert_eq!(check.playlist_entries, 2);
    assert_eq!(check.audio_present, 2, "every track must point at audio that exists");
    assert_eq!(check.analysis_present, 2);
    assert!(check.is_ok());
}

#[test]
fn metadata_survives_the_round_trip() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut t = track(src.path(), 1, "Ébano — Tiësto Remix", "Tiësto");
    t.comment = "8A - C - 128".into();
    t.bpm_x100 = 12_345;
    t.key = "Ebm".into();

    export(dest.path(), &[t], &[]).unwrap();

    let bytes = std::fs::read(dest.path().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let pdb = rbl_pdb::Pdb::parse(&bytes).unwrap();
    let rows = pdb.track_rows(pdb.table(rbl_pdb::PageType::Tracks).unwrap());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Ébano — Tiësto Remix");
    assert_eq!(rows[0].tempo_x100, 12_345);
    assert_eq!(rows[0].comment, "8A - C - 128");

    // The key and artist tables must carry the names, referenced by id.
    let keys = pdb.named_rows(pdb.table(rbl_pdb::PageType::Keys).unwrap());
    assert!(keys.iter().any(|k| k.name == "Ebm"), "{keys:?}");
    let artists = pdb.named_rows(pdb.table(rbl_pdb::PageType::Artists).unwrap());
    assert!(artists.iter().any(|a| a.name == "Tiësto"), "{artists:?}");
}

#[test]
fn every_export_carries_rekordboxs_eight_colours() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    export(dest.path(), &[track(src.path(), 1, "T", "A")], &[]).unwrap();

    let bytes = std::fs::read(dest.path().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let pdb = rbl_pdb::Pdb::parse(&bytes).unwrap();
    let colors = pdb.named_rows(pdb.table(rbl_pdb::PageType::Colors).unwrap());
    let names: Vec<String> = colors.into_iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        vec!["Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple"]
    );
}

#[test]
fn a_missing_source_file_skips_that_track_rather_than_failing_the_export() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let good = track(src.path(), 1, "Present", "A");
    let mut bad = track(src.path(), 2, "Missing", "B");
    bad.source_path = PathBuf::from("/definitely/not/here.mp3");

    let report = export(dest.path(), &[good, bad], &[]).unwrap();
    assert_eq!(report.tracks, 1);
    assert_eq!(report.skipped, vec!["Missing".to_owned()]);
    assert!(verify(dest.path()).unwrap().is_ok());
}

#[test]
fn names_that_fat32_cannot_hold_are_made_safe() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut t = track(src.path(), 1, "Title", "AC/DC: Live?");
    t.album = "Best of *".into();
    export(dest.path(), &[t], &[]).unwrap();

    assert!(dest.path().join("Contents/AC_DC_ Live_/Best of _/source-1.mp3").is_file());
    // And the database must point at exactly where the file landed.
    assert!(verify(dest.path()).unwrap().is_ok());
}

#[test]
fn a_large_export_keeps_every_track_and_playlist_entry() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks: Vec<SourceTrack> = (1..=300)
        .map(|i| track(src.path(), i, &format!("Track {i:03}"), &format!("Artist {}", i % 20)))
        .collect();
    let playlists = vec![
        SourcePlaylist { name: "All".into(), track_indices: (0..300).collect() },
        SourcePlaylist { name: "First ten".into(), track_indices: (0..10).collect() },
    ];

    let report = export(dest.path(), &tracks, &playlists).unwrap();
    assert_eq!(report.tracks, 300);

    let check = verify(dest.path()).unwrap();
    assert_eq!(check.tracks, 300, "tracks must survive spanning pages");
    assert_eq!(check.playlists, 2);
    assert_eq!(check.playlist_entries, 310);
    assert_eq!(check.audio_present, 300);
    assert!(check.missing_audio.is_empty());
}

#[test]
fn an_empty_export_is_refused_rather_than_writing_a_broken_stick() {
    let dest = tempfile::tempdir().unwrap();
    assert!(export(dest.path(), &[], &[]).is_err());
}

#[test]
fn analysis_files_land_where_the_database_says_they_do() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    export(dest.path(), &[track(src.path(), 1, "T", "A")], &[]).unwrap();

    let bytes = std::fs::read(dest.path().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let pdb = rbl_pdb::Pdb::parse(&bytes).unwrap();
    let rows = pdb.track_rows(pdb.table(rbl_pdb::PageType::Tracks).unwrap());
    let analyze = &rows[0].analyze_path;
    assert!(analyze.starts_with("/PIONEER/USBANLZ/"), "{analyze}");
    assert!(analyze.ends_with("ANLZ0000.DAT"), "{analyze}");

    let on_disk = dest.path().join(analyze.trim_start_matches('/'));
    assert!(on_disk.is_file(), "the database points at {analyze}, which does not exist");
    // And it must still be a valid analysis file.
    assert!(rbl_anlz::parse(&std::fs::read(&on_disk).unwrap()).is_ok());
}

#[test]
fn an_export_carries_a_readable_export_library_beside_the_pdb() {
    // A player never opens this file; rekordbox does, to read a stick back.
    let dir = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let tracks = vec![
        track(source.path(), 1, "The Abyss", "ARTBAT"),
        track(source.path(), 2, "Take Me Home", "MORTEN"),
    ];
    let playlists = vec![rbl_export::SourcePlaylist {
        name: "Friday".to_owned(),
        track_indices: vec![1, 0],
    }];

    let report = rbl_export::export(dir.path(), &tracks, &playlists).expect("export");
    assert!(report.one_library, "the report must say it was written");

    let path = dir.path().join("PIONEER/rekordbox/exportLibrary.db");
    assert!(path.exists(), "exportLibrary.db is missing");

    let db = rbl_onelibrary::ExportLibrary::open_read_only(&path).expect("open");
    assert_eq!(db.count("content").unwrap(), 2);
    assert_eq!(db.count("playlist").unwrap(), 1);
    assert_eq!(db.count("playlist_content").unwrap(), 2);

    // The two databases must agree: the playlist order here is the order the
    // export was asked for, not the order the tracks were listed in.
    let mut stmt = db
        .connection()
        .prepare("SELECT content_id FROM playlist_content WHERE playlist_id = 1 ORDER BY sequenceNo")
        .unwrap();
    let order: Vec<i64> =
        stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    assert_eq!(order, vec![2, 1]);

    // And a path in one is the same path as in the other.
    let audio: String = db
        .connection()
        .query_row("SELECT path FROM content WHERE content_id = 1", [], |r| r.get(0))
        .unwrap();
    assert!(audio.starts_with("/Contents/"), "{audio}");
    assert!(dir.path().join(audio.trim_start_matches('/')).exists(), "{audio} is not on the stick");
}

#[test]
fn a_fresh_stick_takes_the_defaults_it_is_given_and_keeps_them_after() {
    use rbl_onelibrary::settings::StickSettings;

    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let tracks = vec![track(src.path(), 1, "All U Need", "TRIODE")];

    // The Preferences window's choices: GENRE turned on as the first
    // category, and BPM as the column beside the title.
    let mut defaults = StickSettings::default();
    let genre = defaults.categories.iter_mut().find(|c| c.name == "GENRE").unwrap();
    genre.visible = true;
    genre.seq = 1;
    defaults.sub_column = Some(5);

    rbl_export::export_with(dest.path(), &tracks, &[], Some(&defaults)).unwrap();
    let db = dest.path().join("PIONEER/rekordbox/exportLibrary.db");
    let written = StickSettings::read(&db).unwrap();
    assert!(written.categories.iter().find(|c| c.name == "GENRE").unwrap().visible);
    assert_eq!(written.sub_column, Some(5));

    // A second export with different defaults changes nothing: the stick's
    // settings are its own now.
    let mut other = StickSettings::default();
    other.sub_column = Some(2);
    rbl_export::export_with(dest.path(), &tracks, &[], Some(&other)).unwrap();
    let kept = StickSettings::read(&db).unwrap();
    assert_eq!(kept.sub_column, Some(5));
    assert!(kept.categories.iter().find(|c| c.name == "GENRE").unwrap().visible);
}
