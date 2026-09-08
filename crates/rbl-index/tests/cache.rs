//! The on-disk snapshot: it must reproduce the library exactly, refuse a stale
//! one, and survive a damaged file without panicking.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use rbl_index::cache::{decode, encode, Fingerprint};
use rbl_index::testing::{add_playlist, library_from, TestTrack};
use rbl_index::{SortColumn, TrackSource, ViewSpec};

fn fingerprint() -> Fingerprint {
    Fingerprint {
        format: rbl_index::cache::FORMAT,
        db_len: 1_234,
        db_modified_ns: 99,
        wal_len: 7,
        wal_modified_ns: 5,
        db_version: 6000,
        content: 42,
    }
}

fn sample() -> Vec<TestTrack> {
    vec![
        TestTrack { id: 1, title: "Zebra", artist: "ARTBAT", bpm_x100: 12800, rating: 4, comment: "hi", ..TestTrack::default() },
        TestTrack { id: 2, title: "apple", artist: "Meduza", bpm_x100: 13000, ..TestTrack::default() },
        TestTrack { id: 3, title: "Ébano", artist: "Tujamo", bpm_x100: 12400, rating: 2, ..TestTrack::default() },
    ]
}

fn built() -> rbl_index::Library {
    let mut lib = library_from(&sample());
    add_playlist(&mut lib, "Set", &[2, 0]);
    lib
}

#[test]
fn a_snapshot_reproduces_the_library_it_came_from() {
    let original = built();
    let restored = decode(&encode(&original, fingerprint()), fingerprint()).expect("decodes");

    assert_eq!(restored.len(), original.len());
    assert_eq!(restored.ids, original.ids);
    for row in 0..original.len() {
        assert_eq!(restored.title.get(row), original.title.get(row));
        assert_eq!(restored.comment.get(row), original.comment.get(row));
        assert_eq!(restored.artist_name(row as u32), original.artist_name(row as u32));
        assert_eq!(restored.rating[row], original.rating[row]);
        assert_eq!(restored.bpm_x100[row], original.bpm_x100[row]);
    }
    assert_eq!(restored.playlists().members, original.playlists().members);
    assert_eq!(restored.playlists().names.get(0), "Set");
}

#[test]
fn the_rebuilt_ranks_and_search_still_work() {
    // They are derived rather than stored, so this is the check that
    // rebuilding them actually happened.
    let restored = decode(&encode(&built(), fingerprint()), fingerprint()).expect("decodes");
    let spec = |sort, query: &str| ViewSpec {
        source: TrackSource::Collection,
        sort,
        descending: false,
        query: query.to_owned(),
    };
    let by_title: Vec<&str> = restored
        .open_view(&spec(SortColumn::Title, ""))
        .rows
        .iter()
        .map(|&r| restored.title.get(r as usize))
        .collect();
    assert_eq!(by_title, vec!["apple", "Ébano", "Zebra"]);
    assert_eq!(restored.open_view(&spec(SortColumn::Title, "artbat")).rows.len(), 1);
}

#[test]
fn a_snapshot_of_a_different_database_is_refused() {
    let bytes = encode(&built(), fingerprint());
    // Every field on its own must be enough to reject it: this is the whole
    // defence against showing somebody a library that has moved on.
    for changed in [
        Fingerprint { content: 43, ..fingerprint() },
        Fingerprint { db_version: 6001, ..fingerprint() },
        Fingerprint { format: rbl_index::cache::FORMAT + 1, ..fingerprint() },
    ] {
        assert!(decode(&bytes, changed).is_none(), "{changed:?} should have been refused");
    }
}

#[test]
fn a_rewritten_log_alone_does_not_throw_the_snapshot_away() {
    // rekordbox rewrites the write-ahead log constantly without changing a
    // row. Refusing the snapshot for that made it useless on every start
    // rekordbox happened to be running for, which is most of them.
    let bytes = encode(&built(), fingerprint());
    for same_content in [
        Fingerprint { wal_len: 8, ..fingerprint() },
        Fingerprint { wal_modified_ns: 999, ..fingerprint() },
        Fingerprint { db_len: 9999, ..fingerprint() },
        Fingerprint { db_modified_ns: 100, ..fingerprint() },
    ] {
        assert!(decode(&bytes, same_content).is_some(), "{same_content:?} should still match");
    }
}

#[test]
fn a_damaged_snapshot_is_refused_rather_than_trusted() {
    let bytes = encode(&built(), fingerprint());
    assert!(decode(&[], fingerprint()).is_none(), "empty");
    assert!(decode(b"not a snapshot at all", fingerprint()).is_none(), "wrong magic");
    // Truncated at every length, which is what a half-written file looks like.
    for cut in (1..bytes.len()).step_by(7) {
        assert!(decode(&bytes[..cut], fingerprint()).is_none(), "truncated to {cut}");
    }
}

#[test]
fn a_length_that_claims_more_than_the_file_holds_does_not_allocate_it() {
    // The failure this guards against is a corrupt count reserving gigabytes
    // before anything notices the file is far too small to hold them.
    let mut bytes = encode(&built(), fingerprint());
    let header = 4 + 4 + 8 + 8 + 8 + 8 + 4;
    // The row count is fine; the first vector's length is the one to poison.
    bytes[header + 8..header + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode(&bytes, fingerprint()).is_none());
}
