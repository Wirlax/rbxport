//! The shell's commands, end to end, against a fixture library.
//!
//! What the webview does — `invoke("create_playlist", …)` and then
//! `fetch_rows` to see the result — this does in Rust: the same command
//! functions, the same `AppState`, a mock Tauri app in place of the window,
//! and a library built in a tempdir from the real schema. The deck plays into
//! a sink the test pulls by hand, so nothing here needs an audio device.
//!
//! Nothing here can reach the installed library. The state is pointed at the
//! fixture once, and every open — reader, writer, reload — goes through it.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rbl_db::fixture::{self, playlist_id, track_id, Shape};
use rbl_db::write::ROOT;
use rbl_db::{Library as Db, OpenMode};
use rbl_deck::{NullSink, Sink};
use rekordbox_lite_lib::commands;
use rekordbox_lite_lib::cues::{self, CueKind};
use rekordbox_lite_lib::details;
use rekordbox_lite_lib::dto::{RowDto, TrackFilterDto, TrackSourceDto, TreeNodeDto, ViewSpecDto};
use rekordbox_lite_lib::player::{Player, TickDto};
use rekordbox_lite_lib::state::AppState;
use rekordbox_lite_lib::ErrorKind;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Listener, Manager, State};

/// The rate the null sink runs at, so a seek in milliseconds is a known
/// number of frames.
const RATE: u32 = 44_100;

struct Shell {
    _dir: tempfile::TempDir,
    app: tauri::App<MockRuntime>,
    /// The engine's output, once a deck command has opened it.
    sink: Arc<Mutex<Option<Arc<NullSink>>>>,
    /// Every `library:changed` generation the interface would have seen.
    changes: Arc<Mutex<Vec<u32>>>,
}

/// A mock app over a fresh fixture, loaded the way `spawn_library_load`
/// loads the real one.
fn shell() -> Shell {
    let dir = tempfile::tempdir().unwrap();
    let location = fixture::build(dir.path(), Shape::default()).expect("build the fixture");

    let state = AppState::with_backups(dir.path().join("backups"));
    let db = Db::open(location.clone(), OpenMode::ReadOnly).expect("open the fixture");
    let (library, _) = rbl_index::load(&db).expect("index the fixture");
    state.set_library(library, false, db.schema().db_version, 0, location);

    let sink: Arc<Mutex<Option<Arc<NullSink>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&sink);
    let player = Player::with_sink(Box::new(move |render, _device| {
        let opened = Arc::new(NullSink::new(RATE, render));
        *slot.lock().unwrap() = Some(Arc::clone(&opened));
        Ok(opened as Arc<dyn Sink>)
    }));

    let app = tauri::test::mock_app();
    app.manage(Arc::new(state));
    app.manage(Arc::new(player));

    let changes: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    app.listen("library:changed", move |event| {
        if let Ok(generation) = serde_json::from_str::<u32>(event.payload()) {
            seen.lock().unwrap().push(generation);
        }
    });

    Shell { _dir: dir, app, sink, changes }
}

/// Runs a command the way the invoke handler does: to completion, on the
/// shell's async runtime.
fn run<T>(fut: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(fut)
}

impl Shell {
    fn handle(&self) -> AppHandle<MockRuntime> {
        self.app.handle().clone()
    }

    fn state(&self) -> State<'_, Arc<AppState>> {
        self.app.state::<Arc<AppState>>()
    }

    fn player(&self) -> State<'_, Arc<Player>> {
        self.app.state::<Arc<Player>>()
    }

    fn tree(&self) -> Vec<TreeNodeDto> {
        run(commands::playlist_tree(self.state())).unwrap()
    }

    /// The tree node called `name`, which a test knows it just made.
    fn node(&self, name: &str) -> TreeNodeDto {
        let tree = self.tree();
        tree.iter()
            .find(|n| n.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("no node named {name} in {:?}", names(&tree)))
    }

    fn has_node(&self, name: &str) -> bool {
        self.tree().iter().any(|n| n.name == name)
    }

    /// Opens a view and returns its id and length.
    fn open(&self, spec: ViewSpecDto) -> (u32, u32) {
        let handle = run(commands::open_view(self.state(), spec)).unwrap();
        (handle.view_id, handle.len)
    }

    fn rows(&self, view_id: u32) -> Vec<RowDto> {
        run(commands::fetch_rows(self.state(), view_id, 0, commands::MAX_ROWS)).unwrap()
    }

    /// The playlist's rows, in its own order.
    fn playlist_rows(&self, playlist: &str) -> Vec<RowDto> {
        let (view, _) = self.open(playlist_spec(playlist));
        self.rows(view)
    }

    fn deck_state(&self) -> TickDto {
        run(commands::deck_state(self.player())).unwrap()
    }

    /// Pulls the sink until the condition holds, or gives up. The engine
    /// decodes on its own thread, so nothing here is instantaneous.
    fn pull_until(&self, what: &str, mut done: impl FnMut(&TickDto) -> bool) -> TickDto {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let tick = self.deck_state();
            if done(&tick) {
                return tick;
            }
            assert!(Instant::now() < deadline, "gave up waiting for {what}: {tick:?}");
            if let Some(sink) = self.sink.lock().unwrap().clone() {
                sink.pull(512);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn names(tree: &[TreeNodeDto]) -> Vec<&str> {
    tree.iter().map(|n| n.name.as_str()).collect()
}

fn titles(rows: &[RowDto]) -> Vec<&str> {
    rows.iter().map(|r| r.title.as_str()).collect()
}

fn ids(rows: &[RowDto]) -> Vec<&str> {
    rows.iter().map(|r| r.id.as_str()).collect()
}

fn collection_spec() -> ViewSpecDto {
    ViewSpecDto {
        source: TrackSourceDto::Collection,
        sort: "title".into(),
        descending: false,
        query: String::new(),
        filter: TrackFilterDto::default(),
    }
}

fn playlist_spec(id: &str) -> ViewSpecDto {
    ViewSpecDto {
        source: TrackSourceDto::Playlist { id: id.to_owned() },
        sort: "trackNo".into(),
        descending: false,
        query: String::new(),
        filter: TrackFilterDto::default(),
    }
}

/// A silent stereo WAV the deck can load and the writer can import.
fn write_wav(path: &Path, seconds: u32) -> PathBuf {
    let frames = RATE * seconds;
    let data_len = frames * 4;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2_u16.to_le_bytes()); // stereo
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(44 + data_len as usize, 0);
    std::fs::write(path, out).unwrap();
    path.to_path_buf()
}

// ------------------------------------------------------------------ reading

#[test]
fn the_summary_and_the_tree_describe_the_loaded_library() {
    let s = shell();
    let summary = run(commands::library_summary(s.state())).unwrap();
    assert_eq!(summary.track_count, 40);
    assert_eq!(summary.playlist_count, 3);
    assert_eq!(summary.db_version, Some(6000));

    let tree = s.tree();
    assert_eq!(tree[0].kind, "allTracks");
    assert_eq!(tree[0].child_count, Some(40));
    assert_eq!(tree[1].kind, "collection");
    let playlists: Vec<&TreeNodeDto> = tree.iter().filter(|n| n.kind == "playlist").collect();
    assert_eq!(playlists.len(), 3);
    assert!(playlists.iter().all(|p| p.depth == 1 && p.child_count == Some(5)));
    assert!(tree.iter().any(|n| n.kind == "histories"), "the fixture records sessions");
}

#[test]
fn a_view_is_a_window_over_rows_the_backend_sorted_and_searched() {
    let s = shell();

    let (view, len) = s.open(collection_spec());
    assert_eq!(len, 40);
    let rows = s.rows(view);
    assert_eq!(rows.len(), 40);
    assert_eq!(rows[0].title, "Track 000");
    assert_eq!(rows[39].title, "Track 039");

    // A window, not the list: the page is what was asked for.
    let page = run(commands::fetch_rows(s.state(), view, 10, 3)).unwrap();
    assert_eq!(titles(&page), ["Track 010", "Track 011", "Track 012"]);
    assert_eq!(page[0].track_no, 11, "numbered from where the window starts");

    // Sorted the other way, on another column, by Rust.
    let (view, _) = s.open(ViewSpecDto { sort: "bpm".into(), descending: true, ..collection_spec() });
    let rows = s.rows(view);
    assert_eq!(rows[0].title, "Track 039", "the fixture's BPM climbs with the index");
    assert!(rows.windows(2).all(|w| w[0].bpm_x100 >= w[1].bpm_x100));

    // Searched by Rust: every token a substring, so "01" is Track 001 and
    // Track 010 to 019.
    let (view, len) = s.open(ViewSpecDto { query: "track 01".into(), ..collection_spec() });
    assert_eq!(len, 11);
    assert!(titles(&s.rows(view)).iter().all(|t| t.contains("01")));

    let (_, none) = s.open(ViewSpecDto { query: "no such track".into(), ..collection_spec() });
    assert_eq!(none, 0);
}

#[test]
fn a_page_past_the_cap_is_refused_before_it_is_built() {
    // The 64 KB response cap is kept by never building more than a page.
    let s = shell();
    let (view, _) = s.open(collection_spec());
    let err = run(commands::fetch_rows(s.state(), view, 0, commands::MAX_ROWS + 1)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Malformed);
}

#[test]
fn a_view_nobody_opened_is_not_found_rather_than_empty() {
    let s = shell();
    let err = run(commands::fetch_rows(s.state(), 999, 0, 10)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

// -------------------------------------------------------- playlist editing

#[test]
fn a_playlist_is_made_filled_reordered_renamed_moved_and_deleted() {
    let s = shell();
    let before = s.tree().len();

    // A folder at the root, and a playlist inside it.
    run(commands::create_folder(s.handle(), s.state(), "Gigs".into(), ROOT.into())).unwrap();
    let gigs = s.node("Gigs");
    assert_eq!(gigs.depth, 1);
    run(commands::create_playlist(s.handle(), s.state(), "Friday".into(), gigs.id.clone())).unwrap();
    let friday = s.node("Friday");
    assert_eq!(friday.depth, 2, "inside the folder");
    assert_eq!(friday.kind, "playlist");
    assert_eq!(s.node("Gigs").kind, "folder", "a folder with something in it");
    assert_eq!(s.playlist_rows(&friday.id).len(), 0);

    // Tracks go in, in the order given, and the view numbers them so.
    let (t1, t2, t3) = (track_id(1), track_id(2), track_id(3));
    run(commands::add_tracks_to_playlist(
        s.handle(),
        s.state(),
        friday.id.clone(),
        vec![t3.clone(), t1.clone(), t2.clone()],
    ))
    .unwrap();
    let rows = s.playlist_rows(&friday.id);
    assert_eq!(ids(&rows), [t3.as_str(), t1.as_str(), t2.as_str()]);
    assert_eq!(rows.iter().map(|r| r.track_no).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(s.node("Friday").child_count, Some(3), "the tree counts them");

    // Reordered, and one taken out closes the gap.
    run(commands::reorder_playlist(
        s.handle(),
        s.state(),
        friday.id.clone(),
        vec![t1.clone(), t2.clone(), t3.clone()],
    ))
    .unwrap();
    assert_eq!(ids(&s.playlist_rows(&friday.id)), [t1.as_str(), t2.as_str(), t3.as_str()]);
    run(commands::remove_tracks_from_playlist(s.handle(), s.state(), friday.id.clone(), vec![t2]))
        .unwrap();
    let rows = s.playlist_rows(&friday.id);
    assert_eq!(ids(&rows), [t1.as_str(), t3.as_str()]);
    assert_eq!(rows.iter().map(|r| r.track_no).collect::<Vec<_>>(), [1, 2]);

    // Renamed, then moved up to the root.
    run(commands::rename_playlist(s.handle(), s.state(), friday.id.clone(), "Saturday".into())).unwrap();
    assert!(!s.has_node("Friday"));
    assert_eq!(s.node("Saturday").id, friday.id, "the same list under a new name");
    run(commands::move_playlist(s.handle(), s.state(), friday.id.clone(), ROOT.into())).unwrap();
    assert_eq!(s.node("Saturday").depth, 1);

    // Deleted, both of them, and the tree is as it was.
    run(commands::delete_playlist(s.handle(), s.state(), gigs.id)).unwrap();
    assert!(!s.has_node("Gigs"));
    assert!(s.has_node("Saturday"), "deleting the folder it left does not take it");
    run(commands::delete_playlist(s.handle(), s.state(), friday.id)).unwrap();
    assert!(!s.has_node("Saturday"));
    assert_eq!(s.tree().len(), before);
}

#[test]
fn every_edit_bumps_the_generation_and_tells_the_interface() {
    let s = shell();
    let (_, _, _, start) = s.state().summary();

    let first = run(commands::create_playlist(s.handle(), s.state(), "One".into(), ROOT.into())).unwrap();
    let second = run(commands::set_track_rating(s.handle(), s.state(), track_id(0), 3)).unwrap();
    assert!(first > start);
    assert!(second > first);
    assert_eq!(s.state().summary().3, second, "the state reports the latest");

    // Both went out as `library:changed`, which is what makes the frontend
    // drop the pages it cached against the old generation.
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.changes.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(*s.changes.lock().unwrap(), vec![first, second]);
}

#[test]
fn an_edit_closes_the_views_that_were_open_over_the_old_library() {
    let s = shell();
    let (view, _) = s.open(playlist_spec(&playlist_id(0)));
    assert_eq!(s.rows(view).len(), 5);

    run(commands::add_tracks_to_playlist(s.handle(), s.state(), playlist_id(0), vec![track_id(30)])).unwrap();

    // The page the frontend held is gone; it reopens against the new tree.
    let err = run(commands::fetch_rows(s.state(), view, 0, 10)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert_eq!(s.playlist_rows(&playlist_id(0)).len(), 6);
}

#[test]
fn a_write_the_library_refuses_is_read_only_to_the_interface_and_changes_nothing() {
    let s = shell();
    let (_, _, _, generation) = s.state().summary();

    let err = run(commands::create_playlist(s.handle(), s.state(), "Orphan".into(), "no-such-folder".into()))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnly, "the status bar shows a refusal, not a crash");
    assert!(!s.has_node("Orphan"));
    assert_eq!(s.state().summary().3, generation, "nothing was reloaded");
    assert!(s.changes.lock().unwrap().is_empty());

    let err = run(commands::set_track_rating(s.handle(), s.state(), track_id(0), 9)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnly);
}

// ----------------------------------------------------------- track editing

#[test]
fn a_rating_a_comment_and_a_colour_show_in_the_rows_after_the_edit() {
    let s = shell();
    let track = track_id(7);

    run(commands::set_track_rating(s.handle(), s.state(), track.clone(), 4)).unwrap();
    run(commands::set_track_comment(s.handle(), s.state(), track.clone(), "opener — long intro".into()))
        .unwrap();
    run(commands::set_track_color(s.handle(), s.state(), track.clone(), Some("pink".into()))).unwrap();

    let (view, _) = s.open(collection_spec());
    let rows = s.rows(view);
    let row = rows.iter().find(|r| r.id == track).expect("the track is still in the collection");
    assert_eq!(row.rating, 4);
    assert_eq!(row.comment, "opener — long intro");

    // And the sort that reads the column sees it too.
    let (view, _) = s.open(ViewSpecDto { sort: "rating".into(), descending: true, ..collection_spec() });
    assert_eq!(s.rows(view)[0].id, track);

    run(commands::set_track_rating(s.handle(), s.state(), track.clone(), 0)).unwrap();
    run(commands::set_track_color(s.handle(), s.state(), track.clone(), None)).unwrap();
    let (view, _) = s.open(collection_spec());
    assert_eq!(s.rows(view).iter().find(|r| r.id == track).unwrap().rating, 0);
}

#[test]
fn the_information_panel_reads_the_record_and_writes_a_field_the_rows_follow() {
    let s = shell();
    let track = track_id(9);

    let record = run(details::track_details(s.state(), track.clone())).unwrap();
    assert_eq!(record.id, track);
    assert_eq!(record.title, "Track 009");
    assert_eq!(record.rating, 0);
    assert_eq!(record.duration_sec, 300);

    run(details::set_track_field(s.handle(), s.state(), track.clone(), "title".into(), "Nine".into())).unwrap();
    run(details::set_track_field(s.handle(), s.state(), track.clone(), "artist".into(), "Somebody".into()))
        .unwrap();
    run(details::set_track_field(s.handle(), s.state(), track.clone(), "year".into(), "2019".into())).unwrap();
    run(commands::set_track_rating(s.handle(), s.state(), track.clone(), 2)).unwrap();

    let record = run(details::track_details(s.state(), track.clone())).unwrap();
    assert_eq!((record.title.as_str(), record.artist.as_str(), record.year, record.rating), ("Nine", "Somebody", 2019, 2));

    // The row in the table, the sort and the search all see the new title.
    let (view, len) = s.open(ViewSpecDto { query: "somebody".into(), ..collection_spec() });
    assert_eq!(len, 1);
    let rows = s.rows(view);
    assert_eq!((rows[0].title.as_str(), rows[0].artist.as_str()), ("Nine", "Somebody"));
    let (view, _) = s.open(ViewSpecDto { sort: "artist".into(), descending: true, ..collection_spec() });
    assert_eq!(s.rows(view)[0].id, track, "the one track with an artist sorts first");

    // A field the writer does not take is refused before anything is opened.
    let err = run(details::set_track_field(s.handle(), s.state(), track.clone(), "bitrate".into(), "320".into()))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnly);
    let err = run(details::set_track_field(s.handle(), s.state(), track, "year".into(), "soon".into())).unwrap_err();
    assert_eq!(err.kind, ErrorKind::ReadOnly);
}

#[test]
fn a_cue_added_through_the_command_is_read_back_and_announced() {
    let s = shell();
    let track = track_id(4);
    let announced: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&announced);
    s.app.listen("cues:changed", move |event| {
        seen.lock().unwrap().push(event.payload().trim_matches('"').to_owned());
    });

    assert!(run(commands::track_cues(s.state(), track.clone())).unwrap().is_empty());

    let hot = run(cues::add_cue(s.handle(), s.state(), track.clone(), CueKind::Hot('B'), 12_000)).unwrap();
    let memory = run(cues::add_cue(s.handle(), s.state(), track.clone(), CueKind::Memory, 30_000)).unwrap();
    let looped = run(cues::add_loop(
        s.handle(),
        s.state(),
        track.clone(),
        CueKind::Memory,
        40_000,
        44_000,
        Some(8),
    ))
    .unwrap();

    let cues = run(commands::track_cues(s.state(), track.clone())).unwrap();
    assert_eq!(cues.len(), 3);
    let by_id = |id: &str| cues.iter().find(|c| c.id == id).unwrap();
    assert_eq!((by_id(&hot).letter.as_str(), by_id(&hot).position_ms, by_id(&hot).memory), ("B", 12_000, false));
    assert!(by_id(&memory).memory);
    assert_eq!((by_id(&looped).position_ms, by_id(&looped).out_ms), (40_000, 44_000));

    // The browser row carries the hot cue's letter without a reload.
    let (view, _) = s.open(collection_spec());
    let rows = s.rows(view);
    let row = rows.iter().find(|r| r.id == track).unwrap();
    assert_eq!(row.hot_cues.iter().map(|c| c.0).collect::<String>(), "B");

    run(cues::move_cue(s.handle(), s.state(), hot.clone(), 15_000)).unwrap();
    run(cues::delete_cue(s.handle(), s.state(), memory.clone())).unwrap();
    let cues = run(commands::track_cues(s.state(), track.clone())).unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues.iter().find(|c| c.id == hot).unwrap().position_ms, 15_000);
    assert!(cues.iter().all(|c| c.id != memory));

    // Every edit named the track, so the decks showing it refetch.
    let deadline = Instant::now() + Duration::from_secs(2);
    while announced.lock().unwrap().len() < 5 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let announced = announced.lock().unwrap();
    assert_eq!(announced.len(), 5);
    assert!(announced.iter().all(|t| *t == track));
}

// ---------------------------------------------------------------- playing

#[test]
fn an_imported_file_goes_into_a_playlist_and_plays_on_a_deck() {
    let s = shell();
    let audio = s._dir.path().join("Silent Two Seconds.wav");
    write_wav(&audio, 2);

    // Imported: the library grows by one, and the row is the file's.
    let report = run(commands::import_files(s.handle(), s.state(), vec![audio.display().to_string()])).unwrap();
    assert_eq!(report.imported, 1);
    assert!(report.skipped.is_empty());
    let id = report.tracks[0].id.clone();
    assert_eq!(run(commands::library_summary(s.state())).unwrap().track_count, 41);
    let (view, _) = s.open(ViewSpecDto { query: "silent".into(), ..collection_spec() });
    let rows = s.rows(view);
    assert_eq!(ids(&rows), [id.as_str()]);
    assert_eq!(rows[0].duration_sec, 2);

    // Into a playlist, and loaded from there.
    run(commands::add_tracks_to_playlist(s.handle(), s.state(), playlist_id(1), vec![id.clone()])).unwrap();
    assert_eq!(s.playlist_rows(&playlist_id(1)).len(), 6);

    // Nothing has opened the audio output yet: a window nobody played in
    // holds no device.
    assert!(s.sink.lock().unwrap().is_none());
    assert!(!s.deck_state().a.loaded);

    run(commands::deck_load(s.handle(), s.state(), s.player(), "a".into(), id.clone())).unwrap();
    let loaded = s.pull_until("the deck to load", |t| t.a.loaded);
    assert_eq!(loaded.sample_rate, RATE);
    assert_eq!(loaded.a.total_frames, u64::from(RATE) * 2);
    assert!(!loaded.a.playing);
    assert_eq!(loaded.a.frames, 0);
    assert!(!loaded.b.loaded, "the other deck is untouched");

    // Playing moves the clock; pausing stops it where it is.
    run(commands::deck_play(s.handle(), s.player(), "a".into())).unwrap();
    let playing = s.pull_until("the playhead to move", |t| t.a.frames > 4_096);
    assert!(playing.a.playing);
    run(commands::deck_pause(s.player(), "a".into())).unwrap();
    s.pull_until("the deck to pause", |t| !t.a.playing);
    // A pause is a fade, and the clock runs to the end of it; after that
    // the deck stays put however much the device pulls.
    let sink = s.sink.lock().unwrap().clone().unwrap();
    for _ in 0..8 {
        sink.pull(512);
    }
    let at = s.deck_state().a.frames;
    for _ in 0..8 {
        sink.pull(512);
    }
    assert_eq!(s.deck_state().a.frames, at, "a paused deck does not drift");

    // A seek lands where it was asked, in frames, whether paused or not.
    run(commands::deck_seek(s.handle(), s.player(), "a".into(), 500.0)).unwrap();
    let sought = s.pull_until("the seek to land", |t| t.a.frames == u64::from(RATE) / 2);
    assert!(sought.a.generation > loaded.a.generation, "the interface snaps rather than eases");

    // Unloaded: the deck is empty again.
    run(commands::deck_unload(s.player(), "a".into())).unwrap();
    let empty = s.pull_until("the deck to unload", |t| !t.a.loaded);
    assert_eq!(empty.a.frames, 0);
}

#[test]
fn a_track_whose_file_is_gone_is_refused_at_load_rather_than_failing_later() {
    let s = shell();
    // The fixture's tracks point at files that do not exist. That is caught
    // when the deck is asked for one, not by the engine mid-play.
    let err = run(commands::deck_load(s.handle(), s.state(), s.player(), "a".into(), "no-such-track".into()))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(s.sink.lock().unwrap().is_none(), "the audio output was not opened for it");
}

#[test]
fn the_two_decks_play_independently_and_the_master_level_is_the_engine_s() {
    let s = shell();
    let a = write_wav(&s._dir.path().join("a.wav"), 1);
    let b = write_wav(&s._dir.path().join("b.wav"), 3);
    let report = run(commands::import_files(
        s.handle(),
        s.state(),
        vec![a.display().to_string(), b.display().to_string()],
    ))
    .unwrap();
    assert_eq!(report.imported, 2);
    let (id_a, id_b) = (report.tracks[0].id.clone(), report.tracks[1].id.clone());

    run(commands::deck_load(s.handle(), s.state(), s.player(), "a".into(), id_a)).unwrap();
    run(commands::deck_load(s.handle(), s.state(), s.player(), "b".into(), id_b)).unwrap();
    s.pull_until("both decks to load", |t| t.a.loaded && t.b.loaded);

    run(commands::set_master_level(s.handle(), s.player(), 0.5)).unwrap();
    assert!((s.deck_state().master - 0.5).abs() < 1e-6);

    run(commands::deck_play(s.handle(), s.player(), "b".into())).unwrap();
    let tick = s.pull_until("deck B to move", |t| t.b.frames > 4_096);
    assert!(tick.b.playing);
    assert!(!tick.a.playing);
    assert_eq!(tick.a.frames, 0, "deck A stays put while B plays");

    // Deck A's one second runs out; deck B is still going.
    run(commands::deck_play(s.handle(), s.player(), "a".into())).unwrap();
    let ended = s.pull_until("deck A to reach its end", |t| !t.a.playing && t.a.frames > 0);
    assert!(ended.b.playing);
    assert!(ended.b.frames > ended.a.frames);
}
