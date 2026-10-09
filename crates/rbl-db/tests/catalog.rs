//! This fork's own: what the MCP server reads, against a fixture library.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used)]

use rbl_db::catalog;
use rbl_db::fixture::{self, playlist_id, track_id, Shape};
use rbl_db::{Library, OpenMode};

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), Shape::default()).expect("build the fixture");
    let library = Library::open(location, OpenMode::ReadOnly).expect("open read-only");
    (dir, library)
}

#[test]
fn every_live_track_comes_with_its_fields() {
    let (_dir, library) = library();
    let tracks = catalog::tracks(library.connection()).unwrap();
    assert_eq!(tracks.len(), Shape::default().tracks);
    let first = &tracks[0];
    assert_eq!(first.id, track_id(0));
    assert_eq!(first.title, "Track 000");
    assert_eq!(first.bpm_x100, 12_800);
    assert_eq!(first.length, 300);
    assert_eq!(first.key, "", "no key is an empty name, not a guess");
}

#[test]
fn the_tree_and_each_playlists_tracks_in_order() {
    let (_dir, library) = library();
    let nodes = catalog::nodes(library.connection()).unwrap();
    assert_eq!(nodes.len(), Shape::default().playlists);
    assert!(nodes.iter().all(|n| n.parent == "root" && n.attribute == 0));

    let members = catalog::memberships(library.connection()).unwrap();
    let first: Vec<&str> = members.iter().filter(|(p, _)| *p == playlist_id(0)).map(|(_, c)| c.as_str()).collect();
    let expected: Vec<String> = (0..Shape::default().tracks_per_playlist).map(track_id).collect();
    assert_eq!(first, expected);
}
