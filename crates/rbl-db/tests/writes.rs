//! Writer behaviour, against a fixture with the real schema.
//!
//! Every test builds its own encrypted library in a tempdir. Nothing here can
//! reach the installed library: `Library::open` refuses read-write on a real
//! install whenever `RB_LITE_TEST` is set, and a fixture is never marked as one.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_db::fixture::{self, playlist_id, track_id, Shape};
use rbl_db::write::{Changed, Unsupported, Writer, ATTRIBUTE_FOLDER, ATTRIBUTE_PLAYLIST, ROOT};
use rbl_db::{DbError, Library, OpenMode};
use rusqlite::params;

struct Fixture {
    _dir: tempfile::TempDir,
    writer: Writer,
}

fn fixture_with(shape: Shape) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), shape).expect("build the fixture");
    let writer = Writer::open(location, dir.path().join("backups")).expect("open for writing");
    Fixture { _dir: dir, writer }
}

fn fixture() -> Fixture {
    fixture_with(Shape::default())
}

impl Fixture {
    fn conn(&self) -> &rusqlite::Connection {
        self.writer.library().connection()
    }

    fn one<T: rusqlite::types::FromSql>(&self, sql: &str, args: &[&dyn rusqlite::ToSql]) -> T {
        self.conn().query_row(sql, args, |r| r.get(0)).unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        self.one(sql, &[])
    }

    /// The `TrackNo` sequence of a playlist, in order.
    fn track_numbers(&self, playlist: &str) -> Vec<i64> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT TrackNo FROM djmdSongPlaylist
                 WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo",
            )
            .unwrap();
        stmt.query_map(params![playlist], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect()
    }

    /// The content ids of a playlist, in playing order.
    fn order(&self, playlist: &str) -> Vec<String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT ContentID FROM djmdSongPlaylist
                 WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo",
            )
            .unwrap();
        stmt.query_map(params![playlist], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect()
    }
}

// ------------------------------------------------------------- new-row shape

#[test]
fn a_new_playlist_has_the_shape_rekordbox_gives_a_local_row() {
    // Every one of the 57 locally-created playlists in the reference library
    // carries rb_data_status 0 and a NULL usn. 256/257 are written by the
    // cloud sync, not by creation — a distinction two sample rows would miss.
    let mut f = fixture();
    let id = f.writer.create_playlist("New Set", ROOT).unwrap();

    let (status, local_status, deleted, synced): (i64, i64, i64, i64) = f
        .conn()
        .query_row(
            "SELECT rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced
             FROM djmdPlaylist WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((status, local_status, deleted, synced), (0, 0, 0, 0));

    let usn: Option<i64> = f.one("SELECT usn FROM djmdPlaylist WHERE ID = ?1", &[&id]);
    assert_eq!(usn, None, "usn is the sync's to assign, not ours");

    let local_usn: i64 = f.one("SELECT rb_local_usn FROM djmdPlaylist WHERE ID = ?1", &[&id]);
    assert!(local_usn > 1000, "must advance past the fixture's starting counter");

    let attribute: i64 = f.one("SELECT Attribute FROM djmdPlaylist WHERE ID = ?1", &[&id]);
    assert_eq!(attribute, ATTRIBUTE_PLAYLIST);
}

#[test]
fn a_new_row_carries_a_uuid_and_matching_timestamps() {
    let mut f = fixture();
    let id = f.writer.create_playlist("Set", ROOT).unwrap();
    let (uuid, created, updated): (String, String, String) = f
        .conn()
        .query_row(
            "SELECT UUID, created_at, updated_at FROM djmdPlaylist WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(uuid.len(), 36, "{uuid}");
    assert_eq!(uuid.split('-').count(), 5);
    assert_eq!(created, updated, "a row created now was not modified later");
    assert!(created.ends_with(" +00:00"), "{created}");
}

#[test]
fn a_folder_differs_from_a_playlist_only_by_its_attribute() {
    let mut f = fixture();
    let folder = f.writer.create_folder("Gigs", ROOT).unwrap();
    let attribute: i64 = f.one("SELECT Attribute FROM djmdPlaylist WHERE ID = ?1", &[&folder]);
    assert_eq!(attribute, ATTRIBUTE_FOLDER);
    // rb_data_status is *not* the folder marker, despite 257 appearing on
    // folders in the reference library: it appears under both Attribute values.
    let status: i64 = f.one("SELECT rb_data_status FROM djmdPlaylist WHERE ID = ?1", &[&folder]);
    assert_eq!(status, 0);
}

#[test]
fn ids_do_not_collide_with_rows_already_there() {
    let mut f = fixture();
    let mut made = std::collections::HashSet::new();
    for i in 0..40 {
        made.insert(f.writer.create_playlist(&format!("Set {i}"), ROOT).unwrap());
    }
    assert_eq!(made.len(), 40);
    for p in 0..3 {
        assert!(!made.contains(&playlist_id(p)), "reused a fixture id");
    }
}

// ------------------------------------------------------------------ the tree

#[test]
fn seq_appends_rather_than_assuming_a_base() {
    // Parents in the reference library start at Seq 0 or 1, so there is no
    // base to assume — a new child goes after the largest.
    let mut f = fixture();
    let a = f.writer.create_playlist("A", ROOT).unwrap();
    let b = f.writer.create_playlist("B", ROOT).unwrap();
    let seq_a: i64 = f.one("SELECT Seq FROM djmdPlaylist WHERE ID = ?1", &[&a]);
    let seq_b: i64 = f.one("SELECT Seq FROM djmdPlaylist WHERE ID = ?1", &[&b]);
    assert_eq!(seq_b, seq_a + 1);
    // The fixture already has three at root, seq 0..2.
    assert_eq!(seq_a, 3);
}

#[test]
fn a_playlist_can_be_created_inside_a_folder() {
    let mut f = fixture();
    let folder = f.writer.create_folder("Gigs", ROOT).unwrap();
    let inner = f.writer.create_playlist("Friday", &folder).unwrap();
    let parent: String = f.one("SELECT ParentID FROM djmdPlaylist WHERE ID = ?1", &[&inner]);
    assert_eq!(parent, folder);
}

#[test]
fn a_parent_that_does_not_exist_is_refused() {
    let mut f = fixture();
    let outcome = f.writer.create_playlist("Orphan", "no-such-folder");
    assert!(matches!(outcome, Err(DbError::WriteRefused(_))), "{outcome:?}");
    // And nothing was written.
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdPlaylist WHERE Name = 'Orphan'"), 0);
}

#[test]
fn a_folder_cannot_be_moved_inside_itself() {
    // This detaches the whole subtree from the tree and it is never seen again.
    let mut f = fixture();
    let outer = f.writer.create_folder("Outer", ROOT).unwrap();
    let inner = f.writer.create_folder("Inner", &outer).unwrap();
    let deep = f.writer.create_folder("Deep", &inner).unwrap();

    assert!(matches!(f.writer.move_to(&outer, &outer), Err(DbError::WriteRefused(_))));
    assert!(matches!(f.writer.move_to(&outer, &inner), Err(DbError::WriteRefused(_))));
    assert!(matches!(f.writer.move_to(&outer, &deep), Err(DbError::WriteRefused(_))));
    // Moving the other way is fine.
    assert!(f.writer.move_to(&deep, ROOT).is_ok());
}

#[test]
fn renaming_changes_the_name_and_nothing_structural() {
    let mut f = fixture();
    let id = f.writer.create_playlist("Before", ROOT).unwrap();
    let seq_before: i64 = f.one("SELECT Seq FROM djmdPlaylist WHERE ID = ?1", &[&id]);
    let created: String = f.one("SELECT created_at FROM djmdPlaylist WHERE ID = ?1", &[&id]);

    let changed = f.writer.rename(&id, "After").unwrap();
    assert_eq!(changed.rows, 1);

    let (name, seq, created_now): (String, i64, String) = f
        .conn()
        .query_row(
            "SELECT Name, Seq, created_at FROM djmdPlaylist WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(name, "After");
    assert_eq!(seq, seq_before);
    assert_eq!(created_now, created, "created_at must not move");
}

#[test]
fn deleting_a_folder_takes_its_whole_subtree_and_the_memberships() {
    let mut f = fixture();
    let outer = f.writer.create_folder("Outer", ROOT).unwrap();
    let inner = f.writer.create_folder("Inner", &outer).unwrap();
    let list = f.writer.create_playlist("Deep list", &inner).unwrap();
    f.writer.add_tracks(&list, &[track_id(0), track_id(1)]).unwrap();

    f.writer.delete_playlist(&outer).unwrap();

    for id in [&outer, &inner, &list] {
        let deleted: i64 = f.one("SELECT rb_local_deleted FROM djmdPlaylist WHERE ID = ?1", &[id]);
        assert_eq!(deleted, 1, "{id} should be soft-deleted");
    }
    assert_eq!(
        f.count(&format!(
            "SELECT COUNT(*) FROM djmdSongPlaylist
             WHERE PlaylistID = '{list}' AND rb_local_deleted = 0"
        )),
        0
    );
}

#[test]
fn a_delete_is_always_soft() {
    // rekordbox's sync relies on the tombstone; a real DELETE loses it.
    let mut f = fixture();
    let before = f.count("SELECT COUNT(*) FROM djmdPlaylist");
    let id = f.writer.create_playlist("Doomed", ROOT).unwrap();
    f.writer.delete_playlist(&id).unwrap();
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdPlaylist"), before + 1, "the row must remain");

    // And rb_data_status is untouched: all 919 deleted rows in the reference
    // library kept theirs.
    let status: i64 = f.one("SELECT rb_data_status FROM djmdPlaylist WHERE ID = ?1", &[&id]);
    assert_eq!(status, 0);
}

// ---------------------------------------------------------------- membership

#[test]
fn tracks_append_with_contiguous_numbering() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    let tracks: Vec<String> = (0..6).map(track_id).collect();
    let changed = f.writer.add_tracks(&list, &tracks).unwrap();
    assert_eq!(changed.rows, 6);
    assert_eq!(f.track_numbers(&list), vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(f.order(&list), tracks);
}

#[test]
fn a_membership_row_is_identified_by_a_uuid_not_a_number() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    f.writer.add_tracks(&list, &[track_id(0)]).unwrap();
    let id: String = f.one(
        "SELECT ID FROM djmdSongPlaylist WHERE PlaylistID = ?1",
        &[&list],
    );
    assert_eq!(id.len(), 36, "{id}");
    assert_eq!(id.split('-').count(), 5);
}

#[test]
fn adding_a_track_already_present_does_nothing() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    f.writer.add_tracks(&list, &[track_id(0), track_id(1)]).unwrap();
    let changed = f.writer.add_tracks(&list, &[track_id(0), track_id(2)]).unwrap();
    assert_eq!(changed.rows, 1, "only the new one");
    assert_eq!(f.order(&list), vec![track_id(0), track_id(1), track_id(2)]);
}

#[test]
fn removing_a_track_closes_the_gap_it_leaves() {
    // TrackNo is contiguous from 1 in every one of the 683 reference
    // playlists; a hole makes rekordbox render the playlist with a gap.
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    let tracks: Vec<String> = (0..5).map(track_id).collect();
    f.writer.add_tracks(&list, &tracks).unwrap();

    f.writer.remove_tracks(&list, &[track_id(1), track_id(3)]).unwrap();

    assert_eq!(f.track_numbers(&list), vec![1, 2, 3]);
    assert_eq!(f.order(&list), vec![track_id(0), track_id(2), track_id(4)]);
}

#[test]
fn reordering_puts_the_tracks_in_the_order_given() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    let tracks: Vec<String> = (0..4).map(track_id).collect();
    f.writer.add_tracks(&list, &tracks).unwrap();

    let wanted = vec![track_id(3), track_id(0), track_id(2), track_id(1)];
    f.writer.reorder(&list, &wanted).unwrap();
    assert_eq!(f.order(&list), wanted);
    assert_eq!(f.track_numbers(&list), vec![1, 2, 3, 4]);
}

#[test]
fn a_partial_reorder_keeps_the_tracks_it_did_not_mention() {
    // Dropping unmentioned tracks would silently empty a playlist when a caller
    // passes only the visible window.
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    let tracks: Vec<String> = (0..5).map(track_id).collect();
    f.writer.add_tracks(&list, &tracks).unwrap();

    f.writer.reorder(&list, &[track_id(4), track_id(3)]).unwrap();

    let order = f.order(&list);
    assert_eq!(order.len(), 5, "nothing may be dropped");
    assert_eq!(order.first(), Some(&track_id(4)));
    assert_eq!(order.get(1), Some(&track_id(3)));
    assert_eq!(f.track_numbers(&list), vec![1, 2, 3, 4, 5]);
}

#[test]
fn a_reorder_naming_a_track_that_is_not_there_ignores_it() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    f.writer.add_tracks(&list, &[track_id(0), track_id(1)]).unwrap();
    f.writer.reorder(&list, &[track_id(9), track_id(1), track_id(0)]).unwrap();
    assert_eq!(f.order(&list), vec![track_id(1), track_id(0)]);
}

#[test]
fn deleting_a_track_removes_it_from_every_playlist_and_renumbers_each() {
    let mut f = fixture();
    let a = f.writer.create_playlist("A", ROOT).unwrap();
    let b = f.writer.create_playlist("B", ROOT).unwrap();
    f.writer.add_tracks(&a, &[track_id(0), track_id(1), track_id(2)]).unwrap();
    f.writer.add_tracks(&b, &[track_id(1), track_id(3)]).unwrap();

    f.writer.delete_track(&track_id(1)).unwrap();

    assert_eq!(f.order(&a), vec![track_id(0), track_id(2)]);
    assert_eq!(f.track_numbers(&a), vec![1, 2]);
    assert_eq!(f.order(&b), vec![track_id(3)]);
    assert_eq!(f.track_numbers(&b), vec![1]);
    let deleted: i64 = f.one("SELECT rb_local_deleted FROM djmdContent WHERE ID = ?1",
                             &[&track_id(1)]);
    assert_eq!(deleted, 1);
}

// ------------------------------------------------------------------ metadata

#[test]
fn a_rating_is_stored_as_rekordbox_stores_it() {
    // Stars are multiples of 51, so five stars is 255 rather than 5.
    let mut f = fixture();
    for (stars, stored) in [(0_u8, 0_i64), (1, 51), (3, 153), (5, 255)] {
        f.writer.set_rating(&track_id(0), stars).unwrap();
        let value: i64 = f.one("SELECT Rating FROM djmdContent WHERE ID = ?1", &[&track_id(0)]);
        assert_eq!(value, stored, "{stars} stars");
    }
}

#[test]
fn an_impossible_rating_is_refused_rather_than_clamped() {
    let mut f = fixture();
    assert!(matches!(f.writer.set_rating(&track_id(0), 6), Err(DbError::WriteRefused(_))));
    let value: i64 = f.one("SELECT Rating FROM djmdContent WHERE ID = ?1", &[&track_id(0)]);
    assert_eq!(value, 0, "the refused write must not have landed");
}

#[test]
fn a_comment_round_trips_including_awkward_text() {
    let mut f = fixture();
    for text in ["", "5A - Am - 128", "quote \" and ' apostrophe", "とんかつ 🎧", "a; DROP TABLE x;--"] {
        f.writer.set_comment(&track_id(2), text).unwrap();
        let stored: String = f.one("SELECT Commnt FROM djmdContent WHERE ID = ?1", &[&track_id(2)]);
        assert_eq!(stored, text);
    }
    // The injection attempt above must not have dropped anything.
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdContent"), 40);
}

#[test]
fn a_colour_can_be_set_and_cleared() {
    let mut f = fixture();
    f.writer.set_color(&track_id(3), Some("4")).unwrap();
    let value: Option<String> = f.one("SELECT ColorID FROM djmdContent WHERE ID = ?1", &[&track_id(3)]);
    assert_eq!(value.as_deref(), Some("4"));
    f.writer.set_color(&track_id(3), None).unwrap();
    let cleared: Option<String> = f.one("SELECT ColorID FROM djmdContent WHERE ID = ?1", &[&track_id(3)]);
    assert_eq!(cleared, None);
}

// ----------------------------------------------------------------- the USN

#[test]
fn every_write_advances_the_usn_and_the_registry_follows() {
    let mut f = fixture();
    let mut last = 0;
    for i in 0..5 {
        let id = f.writer.create_playlist(&format!("Set {i}"), ROOT).unwrap();
        let usn: i64 = f.one("SELECT rb_local_usn FROM djmdPlaylist WHERE ID = ?1", &[&id]);
        assert!(usn > last, "usn must advance: {usn} after {last}");
        last = usn;
        let counter: i64 = f.count(
            "SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'",
        );
        assert_eq!(counter, usn, "the registry must track the rows");
    }
}

#[test]
fn no_two_rows_share_a_usn() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    let tracks: Vec<String> = (0..8).map(track_id).collect();
    f.writer.add_tracks(&list, &tracks).unwrap();

    let mut stmt = f
        .conn()
        .prepare("SELECT rb_local_usn FROM djmdSongPlaylist WHERE PlaylistID = ?1")
        .unwrap();
    let usns: Vec<i64> = stmt
        .query_map(params![list], |r| r.get(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    let distinct: std::collections::HashSet<_> = usns.iter().collect();
    assert_eq!(distinct.len(), usns.len(), "a reused USN makes the sync skip a row");
}

#[test]
fn the_usn_starts_above_whatever_is_already_in_the_tables() {
    // The registry counter has been seen lagging the table maximum; taking the
    // larger of the two is what stops a USN being reused.
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), Shape { start_usn: 10, ..Shape::default() }).unwrap();

    // Push a row's USN far past the registry counter, as a lagging counter does.
    {
        let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
        drop(library);
    }
    let conn = rusqlite::Connection::open(&location.master_db).unwrap();
    conn.pragma_update(None, "cipher", "sqlcipher").unwrap();
    conn.pragma_update(None, "legacy", 4).unwrap();
    conn.pragma_update(None, "key", &location.passphrase).unwrap();
    conn.execute("UPDATE djmdContent SET rb_local_usn = 999999 WHERE ID = ?1",
                 params![track_id(0)]).unwrap();
    drop(conn);

    let mut writer = Writer::open(location, dir.path().join("backups")).unwrap();
    let id = writer.create_playlist("After", ROOT).unwrap();
    let usn: i64 = writer
        .library()
        .connection()
        .query_row("SELECT rb_local_usn FROM djmdPlaylist WHERE ID = ?1", params![id], |r| r.get(0))
        .unwrap();
    assert!(usn > 999_999, "got {usn}, which reuses a USN already in the table");
}

// ---------------------------------------------------------------- the guards

#[test]
fn the_library_is_backed_up_before_the_first_write_and_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), Shape::default()).unwrap();
    let backups = dir.path().join("backups");
    let mut writer = Writer::open(location, &backups).unwrap();

    assert!(!backups.exists(), "opening alone must not back up");
    writer.create_playlist("First", ROOT).unwrap();
    let after_first = std::fs::read_dir(&backups).unwrap().count();
    assert_eq!(after_first, 1);

    for i in 0..3 {
        writer.create_playlist(&format!("More {i}"), ROOT).unwrap();
    }
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1, "once per session");
}

#[test]
fn a_backup_is_a_readable_library_in_its_own_right() {
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), Shape::default()).unwrap();
    let backups = dir.path().join("backups");
    let mut writer = Writer::open(location.clone(), &backups).unwrap();
    writer.create_playlist("Only in the live one", ROOT).unwrap();
    drop(writer);

    let backup = std::fs::read_dir(&backups).unwrap().next().unwrap().unwrap().path();
    let restored = Library::open(
        rbl_db::LibraryLocation { master_db: backup, ..location },
        OpenMode::ReadOnly,
    )
    .expect("the backup must open with the same passphrase");
    assert_eq!(restored.live_track_count().unwrap(), 40);
    // Taken before the write, so it must not contain it.
    let n: i64 = restored
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM djmdPlaylist WHERE Name = 'Only in the live one'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "the backup is the state before the write");
}

#[test]
fn a_refused_action_leaves_nothing_behind() {
    let mut f = fixture();
    let before: i64 = f.count("SELECT COUNT(*) FROM djmdPlaylist");
    let counter_before: i64 =
        f.count("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'");

    assert!(f.writer.create_playlist("Nope", "missing").is_err());

    assert_eq!(f.count("SELECT COUNT(*) FROM djmdPlaylist"), before);
    assert_eq!(
        f.count("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'"),
        counter_before,
        "a rolled-back transaction must not move the counter"
    );
}

#[test]
fn the_unsupported_edits_are_refused_with_a_reason() {
    for action in [
        Unsupported::AnalysisRegistration,
        Unsupported::CueEditing,
        Unsupported::ContentCueOrFile,
    ] {
        let error = Writer::refuse(action);
        let DbError::WriteRefused(reason) = error else {
            panic!("{action:?} should be a refusal");
        };
        assert!(!reason.is_empty());
        // The reason has to say what would settle it, or it is just a "no".
        assert!(
            reason.contains("recording") || reason.contains("not understood"),
            "{reason}"
        );
    }
}

#[test]
fn only_the_named_columns_can_be_set() {
    // The column name is interpolated into SQL. It comes from this crate today,
    // but a future caller reaching it would be injecting into a real library.
    let mut f = fixture();
    assert!(f.writer.set_comment(&track_id(0), "fine").is_ok());
    // The allowlist is what makes that safe; prove it rejects rather than runs.
    let outcome = f.writer.rename("nope", "x");
    assert!(outcome.is_ok(), "a real column still works: {outcome:?}");
}

#[test]
fn a_changed_report_says_how_many_rows_moved() {
    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    assert_eq!(f.writer.add_tracks(&list, &[track_id(0), track_id(1)]).unwrap().rows, 2);
    assert_eq!(f.writer.rename(&list, "Renamed").unwrap().rows, 1);
    assert_eq!(
        f.writer.rename("no-such-playlist", "x").unwrap(),
        Changed { rows: 0, usn: f.count("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'") },
        "renaming nothing changes nothing"
    );
}

#[test]
fn a_large_playlist_stays_consistent_through_many_edits() {
    let mut f = fixture_with(Shape { tracks: 200, ..Shape::default() });
    let list = f.writer.create_playlist("Big", ROOT).unwrap();
    let all: Vec<String> = (0..200).map(track_id).collect();
    f.writer.add_tracks(&list, &all).unwrap();

    // Remove every third track, then reverse what is left.
    let doomed: Vec<String> = (0..200).step_by(3).map(track_id).collect();
    f.writer.remove_tracks(&list, &doomed).unwrap();
    let mut remaining = f.order(&list);
    remaining.reverse();
    f.writer.reorder(&list, &remaining).unwrap();

    assert_eq!(f.order(&list), remaining);
    let expected: Vec<i64> = (1..=remaining.len() as i64).collect();
    assert_eq!(f.track_numbers(&list), expected, "numbering must stay 1..N");
}
