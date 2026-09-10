//! Writer behaviour, against a fixture with the real schema.
//!
//! Every test builds its own encrypted library in a tempdir. Nothing here can
//! reach the installed library: `Library::open` refuses read-write on a real
//! install whenever `RB_LITE_TEST` is set, and a fixture is never marked as one.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_db::fixture::{self, playlist_id, track_id, Shape};
use rbl_db::write::{Changed, TrackField, Unsupported, Writer, ATTRIBUTE_FOLDER, ATTRIBUTE_PLAYLIST, ROOT};
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

// --------------------------------------------------- the information panel

#[test]
fn a_plain_field_is_written_to_its_own_column() {
    let mut f = fixture();
    let t = track_id(4);
    f.writer.set_field(&t, TrackField::Title, "Renamed (Extended Mix)").unwrap();
    f.writer.set_field(&t, TrackField::Lyricist, "Words").unwrap();
    f.writer.set_field(&t, TrackField::Year, "2023").unwrap();
    f.writer.set_field(&t, TrackField::TrackNumber, " 7 ").unwrap();
    f.writer.set_field(&t, TrackField::DiscNumber, "2").unwrap();
    f.writer.set_field(&t, TrackField::PlayCount, "12").unwrap();
    let title: String = f.one("SELECT Title FROM djmdContent WHERE ID = ?1", &[&t]);
    let lyricist: String = f.one("SELECT Lyricist FROM djmdContent WHERE ID = ?1", &[&t]);
    let numbers: (i64, i64, i64, i64) = f
        .conn()
        .query_row(
            "SELECT ReleaseYear, TrackNo, DiscNo, DJPlayCount FROM djmdContent WHERE ID = ?1",
            params![t],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(title, "Renamed (Extended Mix)");
    assert_eq!(lyricist, "Words");
    assert_eq!(numbers, (2023, 7, 2, 12));
}

#[test]
fn a_number_that_is_not_one_is_refused_rather_than_zeroed() {
    let mut f = fixture();
    let t = track_id(4);
    f.writer.set_field(&t, TrackField::Year, "2019").unwrap();
    for bad in ["", "abc", "-1", "20x", "10000"] {
        assert!(
            matches!(f.writer.set_field(&t, TrackField::Year, bad), Err(DbError::WriteRefused(_))),
            "{bad:?} must be refused"
        );
    }
    let year: i64 = f.one("SELECT ReleaseYear FROM djmdContent WHERE ID = ?1", &[&t]);
    assert_eq!(year, 2019, "a refused write leaves the year alone");
}

#[test]
fn a_reference_field_makes_its_lookup_row_once_and_shares_it() {
    let mut f = fixture();
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdArtist"), 0);

    f.writer.set_field(&track_id(0), TrackField::Artist, "TRIODE").unwrap();
    f.writer.set_field(&track_id(1), TrackField::Artist, "TRIODE").unwrap();
    f.writer.set_field(&track_id(1), TrackField::Remixer, "TRIODE").unwrap();
    f.writer.set_field(&track_id(2), TrackField::Composer, "Someone Else").unwrap();
    f.writer.set_field(&track_id(2), TrackField::OriginalArtist, "TRIODE").unwrap();

    // One artist row per distinct name, whatever column pointed at it.
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdArtist"), 2);
    let triode: String = f.one("SELECT ID FROM djmdArtist WHERE Name = 'TRIODE'", &[]);
    for (track, column) in [
        (track_id(0), "ArtistID"),
        (track_id(1), "ArtistID"),
        (track_id(1), "RemixerID"),
        (track_id(2), "OrgArtistID"),
    ] {
        let id: String =
            f.one(&format!("SELECT {column} FROM djmdContent WHERE ID = ?1"), &[&track]);
        assert_eq!(id, triode, "{column} of {track}");
    }

    // The new lookup row has the local-creation shape, as an import's does.
    let (status, usn, uuid): (i64, Option<i64>, String) = f
        .conn()
        .query_row(
            "SELECT rb_data_status, usn, UUID FROM djmdArtist WHERE ID = ?1",
            params![triode],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(usn, None);
    assert_eq!(uuid.len(), 36);
}

#[test]
fn album_genre_and_label_go_through_their_own_tables() {
    let mut f = fixture();
    let t = track_id(6);
    f.writer.set_field(&t, TrackField::Album, "An Album").unwrap();
    f.writer.set_field(&t, TrackField::Genre, "Tech House").unwrap();
    f.writer.set_field(&t, TrackField::Label, "Anjuna").unwrap();
    let (album, genre, label): (String, String, String) = f
        .conn()
        .query_row(
            "SELECT al.Name, g.Name, l.Name FROM djmdContent c
             JOIN djmdAlbum al ON al.ID = c.AlbumID
             JOIN djmdGenre g ON g.ID = c.GenreID
             JOIN djmdLabel l ON l.ID = c.LabelID
             WHERE c.ID = ?1",
            params![t],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((album.as_str(), genre.as_str(), label.as_str()), ("An Album", "Tech House", "Anjuna"));
}

#[test]
fn an_emptied_reference_clears_to_null_and_leaves_the_lookup_row() {
    let mut f = fixture();
    let t = track_id(7);
    f.writer.set_field(&t, TrackField::Artist, "Gone Soon").unwrap();
    f.writer.set_field(&t, TrackField::Artist, "   ").unwrap();
    let artist: Option<String> = f.one("SELECT ArtistID FROM djmdContent WHERE ID = ?1", &[&t]);
    assert_eq!(artist, None, "NULL, as 3,942 of the reference library's artist-less tracks are");
    // Another track may still point at it; nothing is ever hard-deleted.
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdArtist WHERE Name = 'Gone Soon'"), 1);
}

#[test]
fn a_key_is_found_never_made() {
    let mut f = fixture();
    let stamp = rbl_core::time::now();
    f.conn()
        .execute(
            "INSERT INTO djmdKey (ID, ScaleName, Seq, created_at, updated_at) VALUES ('12', 'Fm', 7, ?1, ?1)",
            params![stamp],
        )
        .unwrap();
    let t = track_id(8);
    f.writer.set_field(&t, TrackField::Key, "Fm").unwrap();
    let key: String = f.one("SELECT KeyID FROM djmdContent WHERE ID = ?1", &[&t]);
    assert_eq!(key, "12");

    assert!(matches!(
        f.writer.set_field(&t, TrackField::Key, "H#m"),
        Err(DbError::WriteRefused(_))
    ));
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdKey"), 1, "no key row is invented");
    let still: String = f.one("SELECT KeyID FROM djmdContent WHERE ID = ?1", &[&t]);
    assert_eq!(still, "12");

    f.writer.set_field(&t, TrackField::Key, "").unwrap();
    let cleared: Option<String> = f.one("SELECT KeyID FROM djmdContent WHERE ID = ?1", &[&t]);
    assert_eq!(cleared, None);
}

#[test]
fn every_field_edit_bumps_the_usn_and_the_stamp() {
    let mut f = fixture();
    let t = track_id(9);
    let before: i64 = f.one("SELECT rb_local_usn FROM djmdContent WHERE ID = ?1", &[&t]);
    let changed = f.writer.set_field(&t, TrackField::Artist, "Anyone").unwrap();
    assert_eq!(changed.rows, 1);
    let (usn, updated, created): (i64, String, String) = f
        .conn()
        .query_row(
            "SELECT rb_local_usn, updated_at, created_at FROM djmdContent WHERE ID = ?1",
            params![t],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert!(usn > before);
    assert_eq!(usn, changed.usn);
    assert_ne!(updated, created, "updated_at moves, created_at does not");
    let counter: i64 =
        f.one("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'", &[]);
    assert_eq!(counter, usn);
}

#[test]
fn the_wire_names_round_trip() {
    for (name, field) in [
        ("title", TrackField::Title),
        ("artist", TrackField::Artist),
        ("album", TrackField::Album),
        ("year", TrackField::Year),
        ("trackNumber", TrackField::TrackNumber),
        ("discNumber", TrackField::DiscNumber),
        ("originalArtist", TrackField::OriginalArtist),
        ("composer", TrackField::Composer),
        ("remixer", TrackField::Remixer),
        ("lyricist", TrackField::Lyricist),
        ("playCount", TrackField::PlayCount),
        ("genre", TrackField::Genre),
        ("label", TrackField::Label),
        ("key", TrackField::Key),
    ] {
        assert_eq!(TrackField::parse(name), Some(field));
    }
    // What the panel shows read-only must not be reachable by name either.
    for refused in ["albumArtist", "bpm", "mixName", "message", "hotCueAutoLoad", "publish", ""] {
        assert_eq!(TrackField::parse(refused), None, "{refused}");
    }
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
        Unsupported::CueColour,
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

// -------------------------------------------------------------- relocating

#[test]
fn a_track_can_be_pointed_at_a_moved_file() {
    let f_dir = tempfile::tempdir().unwrap();
    let moved = f_dir.path().join("moved elsewhere.mp3");
    std::fs::write(&moved, b"audio").unwrap();

    let mut f = fixture();
    f.writer.relocate(&track_id(0), &moved).unwrap();

    let (folder, name): (String, String) = f
        .conn()
        .query_row(
            "SELECT FolderPath, FileNameL FROM djmdContent WHERE ID = ?1",
            params![track_id(0)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(folder, moved.to_string_lossy());
    assert_eq!(name, "moved elsewhere.mp3");
}

#[test]
fn relocating_to_something_that_is_not_a_file_is_refused() {
    // Pointing a track at a directory, or at nothing, loses the old location
    // for no gain.
    let dir = tempfile::tempdir().unwrap();
    let mut f = fixture();
    let before: String =
        f.one("SELECT FolderPath FROM djmdContent WHERE ID = ?1", &[&track_id(0)]);

    for bad in [dir.path().to_path_buf(), dir.path().join("no-such-file.mp3")] {
        assert!(matches!(f.writer.relocate(&track_id(0), &bad), Err(DbError::WriteRefused(_))));
    }
    let after: String =
        f.one("SELECT FolderPath FROM djmdContent WHERE ID = ?1", &[&track_id(0)]);
    assert_eq!(after, before, "a refused relocate must not have moved anything");
}

#[test]
fn relocating_leaves_the_analysis_and_memberships_alone() {
    // Everything else keys off the track's id, so moving the audio must not
    // disturb it.
    let f_dir = tempfile::tempdir().unwrap();
    let moved = f_dir.path().join("elsewhere.mp3");
    std::fs::write(&moved, b"audio").unwrap();

    let mut f = fixture();
    let list = f.writer.create_playlist("Set", ROOT).unwrap();
    f.writer.add_tracks(&list, &[track_id(0), track_id(1)]).unwrap();

    f.writer.relocate(&track_id(0), &moved).unwrap();

    assert_eq!(f.order(&list), vec![track_id(0), track_id(1)]);
    let deleted: i64 =
        f.one("SELECT rb_local_deleted FROM djmdContent WHERE ID = ?1", &[&track_id(0)]);
    assert_eq!(deleted, 0);
}

// ------------------------------------------------------------------- cues

#[test]
fn a_cue_is_written_in_the_shape_the_reference_library_shows() {
    // Every column here was settled by counting the reference library's
    // 1,040,598 cues rather than guessed. See the write module's docs.
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(0), 1, 45_000).unwrap();

    let (kind, in_ms, color, index, loop_size, active, out_ms): (
        i64, i64, i64, i64, Option<i64>, i64, Option<i64>,
    ) = f
        .conn()
        .query_row(
            "SELECT Kind, InMsec, Color, ColorTableIndex, BeatLoopSize, ActiveLoop, OutMsec
             FROM djmdCue WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .unwrap();
    assert_eq!(kind, 1, "hot cue A");
    assert_eq!(in_ms, 45_000);
    assert_eq!(color, -1, "hot cues carry -1");
    assert_eq!(index, 21, "the default colour rekordbox writes");
    assert_eq!(loop_size, None, "not a loop");
    assert_eq!(active, 0);
    assert_eq!(out_ms, None);
}

#[test]
fn a_memory_cue_differs_from_a_hot_one_in_its_colour_columns() {
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(0), 0, 1_000).unwrap();
    let (color, index): (i64, i64) = f
        .conn()
        .query_row(
            "SELECT Color, ColorTableIndex FROM djmdCue WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    // 255 and 0, against -1 and 21 for a hot cue.
    assert_eq!(color, 255);
    assert_eq!(index, 0);
}

#[test]
fn a_cue_points_at_its_track_by_id_and_by_uuid() {
    // Both, or rekordbox's sync sees a cue with no owner.
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(2), 5, 10_000).unwrap();
    let (content, content_uuid): (String, String) = f
        .conn()
        .query_row(
            "SELECT ContentID, ContentUUID FROM djmdCue WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(content, track_id(2));
    let expected: String =
        f.one("SELECT UUID FROM djmdContent WHERE ID = ?1", &[&track_id(2)]);
    assert_eq!(content_uuid, expected);
}

#[test]
fn a_cue_carries_a_uuid_of_its_own_and_a_local_usn() {
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(0), 2, 5_000).unwrap();
    let (uuid, usn, sync_usn): (String, i64, Option<i64>) = f
        .conn()
        .query_row(
            "SELECT UUID, rb_local_usn, usn FROM djmdCue WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(uuid.len(), 36);
    assert!(usn > 1000);
    assert_eq!(sync_usn, None, "usn is the sync's to assign");
}

#[test]
fn kind_four_is_refused_because_rekordbox_does_not_use_it() {
    let mut f = fixture();
    assert!(matches!(f.writer.add_cue(&track_id(0), 4, 0), Err(DbError::WriteRefused(_))));
    assert!(matches!(f.writer.add_cue(&track_id(0), 18, 0), Err(DbError::WriteRefused(_))));
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdCue"), 0);
}

#[test]
fn a_cue_on_a_track_that_is_not_there_is_refused() {
    let mut f = fixture();
    assert!(matches!(f.writer.add_cue("no-such-track", 1, 0), Err(DbError::WriteRefused(_))));
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdCue"), 0);
}

#[test]
fn a_cue_moves_and_soft_deletes() {
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(0), 1, 1_000).unwrap();

    f.writer.move_cue(&id, 90_000).unwrap();
    let at: i64 = f.one("SELECT InMsec FROM djmdCue WHERE ID = ?1", &[&id]);
    assert_eq!(at, 90_000);

    f.writer.delete_cue(&id).unwrap();
    let deleted: i64 = f.one("SELECT rb_local_deleted FROM djmdCue WHERE ID = ?1", &[&id]);
    assert_eq!(deleted, 1);
    // Soft, as everywhere else: the row stays for the sync's sake.
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdCue"), 1);
}

#[test]
fn every_hot_cue_slot_rekordbox_uses_can_be_written() {
    // 1-3 and 5-17: sixteen slots, A to P, with 4 unused.
    let mut f = fixture();
    for kind in [1_u8, 2, 3, 5, 6, 7, 8, 9, 10, 17] {
        f.writer.add_cue(&track_id(0), kind, u32::from(kind) * 1000).unwrap();
    }
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0"), 10);
}

#[test]
fn a_cue_id_is_a_number_under_2_to_the_32_like_rekordbox_s_own() {
    // Every one of the reference library's 1,041,056 cue ids is a decimal
    // string, the largest 4,294,966,064, and not one is a UUID. The index
    // keeps them as u32, so a UUID here would be a cue that cannot be edited.
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(0), 0, 1_000).unwrap();
    let value: u64 = id.parse().expect("a decimal id");
    assert!(value > 0 && value < (1 << 32), "{id}");
    let looped = f.writer.add_loop(&track_id(0), 0, 2_000, 4_000, 4).unwrap();
    assert!(looped.parse::<u32>().is_ok(), "{looped}");
}

#[test]
fn a_cue_knows_its_track_until_it_is_deleted() {
    let mut f = fixture();
    let id = f.writer.add_cue(&track_id(4), 0, 1_000).unwrap();
    assert_eq!(f.writer.cue_owner(&id).unwrap(), Some(track_id(4)));
    assert_eq!(f.writer.cue_owner("no-such-cue").unwrap(), None);
    f.writer.delete_cue(&id).unwrap();
    assert_eq!(f.writer.cue_owner(&id).unwrap(), None, "a deleted cue has no owner to report");
}

#[test]
fn a_custom_cue_colour_is_still_refused() {
    // What RGB an index past the default means is unknown.
    let DbError::WriteRefused(reason) = Writer::refuse(Unsupported::CueColour) else {
        panic!("a custom cue colour should be a refusal");
    };
    // The reason has to say what would settle it, or it is just a "no".
    assert!(reason.contains("recording"), "{reason}");
}

// ----------------------------------------------------------------- import

/// A minimal but genuine WAV, so the tag reader has something real to open.
fn write_wav(path: &std::path::Path, seconds: u32) {
    let rate = 44_100_u32;
    let samples = rate * seconds;
    let data_len = samples * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1_u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(44 + data_len as usize, 0);
    std::fs::write(path, out).unwrap();
}

#[test]
fn a_file_is_imported_in_the_shape_a_local_row_has() {
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Some Track.wav");
    write_wav(&path, 2);

    let mut f = fixture();
    let id = f.writer.import_file(&path).unwrap();

    let (status, local_status, synced, usn): (i64, i64, i64, Option<i64>) = f
        .conn()
        .query_row(
            "SELECT rb_data_status, rb_local_data_status, rb_local_synced, usn
             FROM djmdContent WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    // The shape all 634 locally-created tracks in the reference library have.
    assert_eq!((status, local_status, synced), (0, 0, 0));
    assert_eq!(usn, None, "usn is the sync's to assign");
}

#[test]
fn an_imported_track_leaves_analysed_unset() {
    // Every track in the reference library has been analysed, so it cannot
    // show what the field holds before analysis. NULL asserts nothing.
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Track.wav");
    write_wav(&path, 1);

    let mut f = fixture();
    let id = f.writer.import_file(&path).unwrap();
    let analysed: Option<i64> =
        f.one("SELECT Analysed FROM djmdContent WHERE ID = ?1", &[&id]);
    assert_eq!(analysed, None);
}

#[test]
fn an_untagged_file_takes_its_filename_as_its_title() {
    // A row with no title is unusable, and the filename is what someone
    // actually recognises.
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Bicep - Glue.wav");
    write_wav(&path, 1);

    let mut f = fixture();
    let id = f.writer.import_file(&path).unwrap();
    let title: String = f.one("SELECT Title FROM djmdContent WHERE ID = ?1", &[&id]);
    assert_eq!(title, "Bicep - Glue");
}

#[test]
fn an_import_records_where_the_file_is_and_how_long_it_runs() {
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Track.wav");
    write_wav(&path, 3);

    let mut f = fixture();
    let id = f.writer.import_file(&path).unwrap();
    let (folder, name, length): (String, String, i64) = f
        .conn()
        .query_row(
            "SELECT FolderPath, FileNameL, Length FROM djmdContent WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(folder, path.to_string_lossy());
    assert_eq!(name, "Track.wav");
    assert_eq!(length, 3);
}

#[test]
fn importing_the_same_file_twice_is_refused() {
    // One file with two rows leaves every playlist pointing at the wrong one.
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Track.wav");
    write_wav(&path, 1);

    let mut f = fixture();
    f.writer.import_file(&path).unwrap();
    assert!(matches!(f.writer.import_file(&path), Err(DbError::WriteRefused(_))));
    assert_eq!(
        f.count("SELECT COUNT(*) FROM djmdContent WHERE rb_local_deleted = 0"),
        41,
        "forty fixture tracks and the one import"
    );
}

#[test]
fn importing_something_that_is_not_audio_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, b"x").unwrap();

    let mut f = fixture();
    assert!(matches!(f.writer.import_file(&text), Err(DbError::WriteRefused(_))));
    assert!(matches!(
        f.writer.import_file(&dir.path().join("missing.wav")),
        Err(DbError::WriteRefused(_))
    ));
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdContent"), 40);
}

#[test]
fn an_imported_track_can_go_straight_into_a_playlist() {
    let audio = tempfile::tempdir().unwrap();
    let path = audio.path().join("Track.wav");
    write_wav(&path, 1);

    let mut f = fixture();
    let id = f.writer.import_file(&path).unwrap();
    let list = f.writer.create_playlist("New", ROOT).unwrap();
    f.writer.add_tracks(&list, std::slice::from_ref(&id)).unwrap();
    assert_eq!(f.order(&list), vec![id]);
}

#[test]
fn a_loop_records_its_length_in_beats() {
    // BeatLoopSize is (beats << 16) | 1. Every value in the reference library
    // fits — 65537, 524289, 1048577, 2097153, 4194305 are 1, 8, 16, 32, 64 —
    // and the one loop whose track is still live carries 262145, four beats,
    // over an In/Out span measuring exactly four beats at its own BPM.
    let mut f = fixture();
    let id = f.writer.add_loop(&track_id(0), 1, 10_000, 11_739, 4).unwrap();

    let (out, size): (i64, i64) = f
        .conn()
        .query_row(
            "SELECT OutMsec, BeatLoopSize FROM djmdCue WHERE ID = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(out, 11_739);
    assert_eq!(size, 262_145, "four beats");
    assert_eq!(size >> 16, 4);
    assert_eq!(size & 0xFFFF, 1, "the low half is 1 on every value observed");
}

#[test]
fn every_loop_length_the_library_uses_round_trips() {
    let mut f = fixture();
    for (beats, expected) in [(1_u16, 65_537_i64), (8, 524_289), (16, 1_048_577),
                              (32, 2_097_153), (64, 4_194_305)] {
        let id = f.writer.add_loop(&track_id(1), 0, 0, 1000, beats).unwrap();
        let size: i64 = f.one("SELECT BeatLoopSize FROM djmdCue WHERE ID = ?1", &[&id]);
        assert_eq!(size, expected, "{beats} beats");
    }
}

#[test]
fn a_loop_with_no_stated_length_leaves_the_field_zero() {
    // Most of the library's loops do this: the length is implied by In and Out.
    let mut f = fixture();
    let id = f.writer.add_loop(&track_id(0), 0, 1000, 2000, 0).unwrap();
    let size: i64 = f.one("SELECT BeatLoopSize FROM djmdCue WHERE ID = ?1", &[&id]);
    assert_eq!(size, 0);
}

#[test]
fn a_loop_that_ends_before_it_starts_is_refused() {
    let mut f = fixture();
    assert!(matches!(f.writer.add_loop(&track_id(0), 1, 5000, 5000, 4), Err(DbError::WriteRefused(_))));
    assert!(matches!(f.writer.add_loop(&track_id(0), 1, 5000, 1000, 4), Err(DbError::WriteRefused(_))));
    assert_eq!(f.count("SELECT COUNT(*) FROM djmdCue"), 0);
}
