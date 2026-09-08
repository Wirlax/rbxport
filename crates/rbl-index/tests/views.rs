//! View semantics: ordering, search, windowing and the edge cases that would
//! otherwise show up as a panic in front of the user.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use rbl_index::testing::{add_playlist, library_from, TestTrack};
use rbl_index::{SortColumn, TrackSource, ViewSpec};

fn track(id: u64, title: &'static str, artist: &'static str, bpm: u32) -> TestTrack {
    TestTrack {
        id,
        title,
        artist,
        bpm_x100: bpm,
        length_sec: 200 + id as u32,
        ..TestTrack::default()
    }
}

fn sample() -> Vec<TestTrack> {
    vec![
        track(1, "Zebra", "ARTBAT", 12800),
        track(2, "apple", "Meduza", 13000),
        track(3, "Ébano", "Tujamo", 12400),
        track(4, "Banana", "artbat", 0),
        track(5, "Cherry (Extended Mix)", "Kryder", 14000),
    ]
}

fn spec(sort: SortColumn, descending: bool, query: &str) -> ViewSpec {
    ViewSpec { source: TrackSource::Collection, sort, descending, query: query.to_owned() }
}

#[test]
fn sorts_by_title_case_and_accent_insensitively() {
    let lib = library_from(&sample());
    let view = lib.open_view(&spec(SortColumn::Title, false, ""));
    let titles: Vec<&str> = view.rows.iter().map(|&r| lib.title.get(r as usize)).collect();
    assert_eq!(titles, ["apple", "Banana", "Cherry (Extended Mix)", "Ébano", "Zebra"]);
}

#[test]
fn descending_is_the_exact_reverse_of_ascending() {
    let lib = library_from(&sample());
    let asc = lib.open_view(&spec(SortColumn::Title, false, ""));
    let desc = lib.open_view(&spec(SortColumn::Title, true, ""));
    let mut reversed = desc.rows.clone();
    reversed.reverse();
    assert_eq!(asc.rows, reversed);
}

#[test]
fn sorts_numerically_not_lexically() {
    let lib = library_from(&sample());
    let view = lib.open_view(&spec(SortColumn::Bpm, false, ""));
    let bpms: Vec<u32> = view.rows.iter().map(|&r| lib.bpm_x100[r as usize]).collect();
    assert_eq!(bpms, [0, 12400, 12800, 13000, 14000]);
}

#[test]
fn search_is_case_and_accent_insensitive() {
    let lib = library_from(&sample());
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "ARTBAT")).len(), 2);
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "ebano")).len(), 1);
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "ÉBANO")).len(), 1);
}

#[test]
fn search_requires_every_token() {
    let lib = library_from(&sample());
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "cherry extended")).len(), 1);
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "cherry zebra")).len(), 0);
}

#[test]
fn search_ignores_punctuation() {
    let lib = library_from(&sample());
    assert_eq!(lib.open_view(&spec(SortColumn::Title, false, "(extended mix)")).len(), 1);
}

#[test]
fn refining_a_view_matches_searching_from_scratch() {
    let lib = library_from(&sample());
    let broad = lib.open_view(&spec(SortColumn::Title, false, "a"));
    let refined = lib.refine(&broad, "artbat");
    let direct = lib.open_view(&spec(SortColumn::Title, false, "artbat"));
    let mut a = refined.rows.clone();
    let mut b = direct.rows.clone();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
}

#[test]
fn clearing_the_query_restores_the_previous_set() {
    let lib = library_from(&sample());
    let broad = lib.open_view(&spec(SortColumn::Title, false, "a"));
    assert_eq!(lib.refine(&broad, "").rows, broad.rows);
}

#[test]
fn window_clamps_instead_of_panicking() {
    let lib = library_from(&sample());
    let view = lib.open_view(&spec(SortColumn::Title, false, ""));
    assert_eq!(view.window(0, 2).len(), 2);
    assert_eq!(view.window(3, 100).len(), 2);
    assert_eq!(view.window(500, 64).len(), 0);
    assert_eq!(view.window(0, 0).len(), 0);
    assert_eq!(view.window(usize::MAX, 64).len(), 0);
}

#[test]
fn ids_in_range_works_in_either_direction_and_clamps() {
    let lib = library_from(&sample());
    let view = lib.open_view(&spec(SortColumn::Title, false, ""));
    let forward = lib.ids_in_range(&view, 1, 3);
    let backward = lib.ids_in_range(&view, 3, 1);
    assert_eq!(forward, backward);
    assert_eq!(forward.len(), 3);
    assert_eq!(lib.ids_in_range(&view, 0, 999).len(), 5);
}

#[test]
fn a_playlist_view_holds_only_its_own_rows() {
    let mut lib = library_from(&sample());
    let pl = add_playlist(&mut lib, "Set", &[4, 0, 2]);
    let view = lib.open_view(&ViewSpec {
        source: TrackSource::Playlist(pl),
        sort: SortColumn::Title,
        descending: false,
        query: String::new(),
    });
    let titles: Vec<&str> = view.rows.iter().map(|&r| lib.title.get(r as usize)).collect();
    assert_eq!(titles, ["Cherry (Extended Mix)", "Ébano", "Zebra"]);
}

#[test]
fn searching_within_a_playlist_stays_within_it() {
    let mut lib = library_from(&sample());
    let pl = add_playlist(&mut lib, "Set", &[0, 1]);
    let view = lib.open_view(&ViewSpec {
        source: TrackSource::Playlist(pl),
        sort: SortColumn::Title,
        descending: false,
        query: "artbat".to_owned(),
    });
    // Row 3 ("Banana" / "artbat") also matches, but is not in this playlist.
    assert_eq!(view.len(), 1);
}

#[test]
fn an_unknown_playlist_index_yields_an_empty_view_rather_than_panicking() {
    let lib = library_from(&sample());
    let view = lib.open_view(&ViewSpec {
        source: TrackSource::Playlist(999),
        sort: SortColumn::Title,
        descending: false,
        query: String::new(),
    });
    assert!(view.is_empty());
}

#[test]
fn an_empty_library_sorts_and_searches_without_panicking() {
    let lib = library_from(&[]);
    let view = lib.open_view(&spec(SortColumn::Title, false, ""));
    assert!(view.is_empty());
    assert!(lib.open_view(&spec(SortColumn::Bpm, true, "anything")).is_empty());
    assert!(lib.ids_in_range(&view, 0, 10).is_empty());
    assert_eq!(view.window(0, 64).len(), 0);
}

#[test]
fn every_sort_column_produces_a_full_permutation() {
    let lib = library_from(&sample());
    for column in [
        SortColumn::TrackNo, SortColumn::Title, SortColumn::Artist, SortColumn::Album,
        SortColumn::Genre, SortColumn::Label, SortColumn::Key, SortColumn::Bpm,
        SortColumn::Duration, SortColumn::Rating, SortColumn::DateAdded, SortColumn::ReleaseDate,
    ] {
        for descending in [false, true] {
            let view = lib.open_view(&spec(column, descending, ""));
            let mut rows = view.rows.clone();
            rows.sort_unstable();
            rows.dedup();
            assert_eq!(rows.len(), 5, "{column:?} desc={descending} dropped or duplicated rows");
        }
    }
}

#[test]
fn hot_cue_letters_follow_rekordbox_s_kind_numbering() {
    use rbl_index::Cue;
    // 1,2,3 then 5 — kind 4 is unused, which is why D is 5. Counted across all
    // 1,040,598 cues in the reference library.
    let letter = |kind: u8| Cue { position_ms: 0, kind }.hot_letter();
    assert_eq!(letter(0), None, "kind 0 is a memory cue");
    assert_eq!(letter(1), Some('A'));
    assert_eq!(letter(2), Some('B'));
    assert_eq!(letter(3), Some('C'));
    assert_eq!(letter(4), None, "kind 4 is not used");
    assert_eq!(letter(5), Some('D'));
    assert_eq!(letter(6), Some('E'));
    assert_eq!(letter(9), Some('H'));
    // rekordbox 7 has sixteen hot cues, not the eight recorded before.
    assert_eq!(letter(10), Some('I'));
    assert_eq!(letter(17), Some('P'));
    assert_eq!(letter(18), None, "past the sixteenth");
    assert_eq!(letter(255), None);
}

#[test]
fn a_memory_cue_is_distinguishable_from_a_hot_one() {
    use rbl_index::Cue;
    assert!(Cue { position_ms: 0, kind: 0 }.is_memory());
    assert!(!Cue { position_ms: 0, kind: 1 }.is_memory());
}
