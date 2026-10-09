//! This fork's own: `Writer::add_mini_sets` against a fixture library.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_db::fixture::{self, playlist_id, track_id, Shape};
use rbl_db::mini_sets::Separator;
use rbl_db::write::{Writer, ROOT};
use rbl_db::DbError;
use rusqlite::params;

struct Fixture {
    _dir: tempfile::TempDir,
    writer: Writer,
}

/// Thirty tracks, two empty playlists, and tracks 20 to 25 retitled as the
/// separators `SEPARATORBREMSEN`, `100`, `099`, `098`, `097` and `096`.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let shape = Shape { tracks: 30, playlists: 2, tracks_per_playlist: 0, history_sessions: 0, start_usn: 1000 };
    let location = fixture::build(dir.path(), shape).expect("build the fixture");
    let writer = Writer::open(location, dir.path().join("backups")).expect("open for writing");
    let f = Fixture { _dir: dir, writer };
    for (index, separator) in (20..).zip([
        Separator::Head,
        Separator::Numbered(100),
        Separator::Numbered(99),
        Separator::Numbered(98),
        Separator::Numbered(97),
        Separator::Numbered(96),
    ]) {
        f.writer
            .library()
            .connection()
            .execute("UPDATE djmdContent SET Title = ?1 WHERE ID = ?2", params![separator.title(), track_id(index)])
            .unwrap();
    }
    f
}

fn ids(indices: &[usize]) -> Vec<String> {
    indices.iter().map(|&i| track_id(i)).collect()
}

fn blocks(blocks: &[&[usize]]) -> Vec<Vec<String>> {
    blocks.iter().map(|b| ids(b)).collect()
}

#[derive(Debug, PartialEq)]
struct Member {
    row: String,
    content: String,
    track_no: i64,
    usn: i64,
}

impl Fixture {
    fn members(&self, playlist: &str) -> Vec<Member> {
        let conn = self.writer.library().connection();
        let mut stmt = conn
            .prepare(
                "SELECT ID, ContentID, TrackNo, rb_local_usn FROM djmdSongPlaylist
                 WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo",
            )
            .unwrap();
        stmt.query_map(params![playlist], |r| {
            Ok(Member { row: r.get(0)?, content: r.get(1)?, track_no: r.get(2)?, usn: r.get(3)? })
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    fn contents(&self, playlist: &str) -> Vec<String> {
        self.members(playlist).into_iter().map(|m| m.content).collect()
    }

    fn counter(&self) -> i64 {
        self.writer
            .library()
            .connection()
            .query_row("SELECT int_1 FROM agentRegistry WHERE registry_id = 'localUpdateCount'", [], |r| r.get(0))
            .unwrap()
    }
}

fn refusal<T: std::fmt::Debug>(result: rbl_db::Result<T>) -> String {
    match result {
        Err(DbError::WriteRefused(reason)) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_empty_playlist_gets_the_head_separator_then_100() {
    let mut f = fixture();
    let playlist = playlist_id(0);
    let (changed, placed) = f.writer.add_mini_sets(&playlist, &blocks(&[&[0, 1], &[2]])).unwrap();

    assert_eq!(f.contents(&playlist), ids(&[20, 0, 1, 21, 2]));
    let numbers: Vec<i64> = f.members(&playlist).iter().map(|m| m.track_no).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5]);
    let titles: Vec<String> = placed.iter().map(|p| p.separator.title()).collect();
    assert_eq!(titles, ["SEPARATORBREMSEN", "SEPARATORBREMSEN 100"]);
    assert!(placed.iter().all(|p| !p.reserved));
    assert_eq!(changed.rows, 5);
    assert_eq!(changed.usn, f.counter(), "the counter moves with the rows");
}

#[test]
fn a_block_fills_the_next_empty_separator_and_the_rows_already_there_keep_their_ids() {
    let mut f = fixture();
    let playlist = playlist_id(0);
    f.writer.add_tracks(&playlist, &ids(&[20, 0, 21, 22])).unwrap();
    let before = f.members(&playlist);

    let (_, placed) = f.writer.add_mini_sets(&playlist, &blocks(&[&[5, 6]])).unwrap();

    assert_eq!(f.contents(&playlist), ids(&[20, 0, 21, 5, 6, 22]));
    assert_eq!(placed[0].separator, Separator::Numbered(100));
    assert!(placed[0].reserved);
    let after = f.members(&playlist);
    // The three rows before the block did not move: same row, same USN.
    assert_eq!(after[..3], before[..3]);
    // `099` moved from 4 to 6: the same row, renumbered under a new USN.
    let moved = &after[5];
    assert_eq!(moved.row, before[3].row);
    assert_eq!(moved.track_no, 6);
    assert!(moved.usn > before[3].usn);
}

#[test]
fn a_refused_block_leaves_the_playlist_and_the_counter_alone() {
    let mut f = fixture();
    let playlist = playlist_id(0);
    f.writer.add_tracks(&playlist, &ids(&[20, 0])).unwrap();
    let before = f.members(&playlist);
    let counter = f.counter();

    let reason = refusal(f.writer.add_mini_sets(&playlist, &blocks(&[&[1], &[0]])));

    assert!(reason.contains("already in the playlist"), "{reason}");
    assert_eq!(f.members(&playlist), before);
    assert_eq!(f.counter(), counter);
}

#[test]
fn an_unknown_track_or_a_folder_is_refused() {
    let mut f = fixture();
    let reason = refusal(f.writer.add_mini_sets(&playlist_id(0), &[vec!["999999".to_owned()]]));
    assert_eq!(reason, "no track 999999");

    let folder = f.writer.create_folder("Sets", ROOT).unwrap();
    let reason = refusal(f.writer.add_mini_sets(&folder, &blocks(&[&[0]])));
    assert!(reason.contains("is a folder"), "{reason}");
}
