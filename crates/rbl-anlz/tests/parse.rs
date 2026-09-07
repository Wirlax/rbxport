//! Container and tag decoding, including the malformed inputs that must
//! degrade rather than panic.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used)]

use rbl_anlz::{parse, Anlz, AnlzError, Section};
use rbl_core::FourCc;

/// Builds a PMAI file from (tag, body) pairs.
fn build(sections: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (tag, payload) in sections {
        let len_tag = 12 + payload.len();
        body.extend_from_slice(*tag);
        body.extend_from_slice(&12_u32.to_be_bytes()); // len_header
        body.extend_from_slice(&(len_tag as u32).to_be_bytes());
        body.extend_from_slice(payload);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"PMAI");
    out.extend_from_slice(&28_u32.to_be_bytes()); // len_header
    out.extend_from_slice(&((28 + body.len()) as u32).to_be_bytes());
    out.extend_from_slice(&[0; 16]); // header padding to 28 bytes
    out.extend_from_slice(&body);
    out
}

fn beat_grid(beats: &[(u16, u16, u32)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&0_u32.to_be_bytes());
    b.extend_from_slice(&0x0008_0000_u32.to_be_bytes());
    b.extend_from_slice(&(beats.len() as u32).to_be_bytes());
    for (number, tempo, time) in beats {
        b.extend_from_slice(&number.to_be_bytes());
        b.extend_from_slice(&tempo.to_be_bytes());
        b.extend_from_slice(&time.to_be_bytes());
    }
    b
}

fn path_tag(text: &str) -> Vec<u8> {
    let mut utf16: Vec<u8> = Vec::new();
    for unit in text.encode_utf16() {
        utf16.extend_from_slice(&unit.to_be_bytes());
    }
    utf16.extend_from_slice(&[0, 0]); // NUL terminator
    let mut b = (utf16.len() as u32).to_be_bytes().to_vec();
    b.extend_from_slice(&utf16);
    b
}

#[test]
fn rejects_a_file_without_the_pmai_magic() {
    assert!(matches!(parse(b"NOPE\0\0\0\0"), Err(AnlzError::NotAnlz)));
}

#[test]
fn rejects_a_truncated_header_without_panicking() {
    assert!(parse(b"PMAI").is_err());
    assert!(parse(b"PMAI\0\0").is_err());
}

#[test]
fn an_empty_file_is_an_error_not_a_panic() {
    assert!(parse(&[]).is_err());
}

#[test]
fn reads_a_beat_grid() {
    let file = build(&[(b"PQTZ", beat_grid(&[(1, 12800, 0), (2, 12800, 469), (3, 12800, 938)]))]);
    let anlz = parse(&file).unwrap();
    let beats = anlz.beat_grid().unwrap();
    assert_eq!(beats.len(), 3);
    assert_eq!(beats[0].beat_number, 1);
    assert_eq!(beats[1].tempo_x100, 12800);
    assert_eq!(beats[2].time_ms, 938);
}

#[test]
fn a_truncated_beat_grid_keeps_the_beats_it_could_read() {
    // Declares four beats but supplies two.
    let mut body = beat_grid(&[(1, 12800, 0), (2, 12800, 469)]);
    body[8..12].copy_from_slice(&4_u32.to_be_bytes());
    let anlz = parse(&build(&[(b"PQTZ", body)])).unwrap();
    assert_eq!(anlz.beat_grid().unwrap().len(), 2);
}

#[test]
fn reads_a_utf16_path() {
    let anlz = parse(&build(&[(b"PPTH", path_tag("/Users/dj/Music/Track Ébano.mp3"))])).unwrap();
    assert_eq!(anlz.path().unwrap(), "/Users/dj/Music/Track Ébano.mp3");
}

#[test]
fn keeps_unknown_tags_verbatim_instead_of_dropping_them() {
    let anlz = parse(&build(&[(b"PZZZ", vec![1, 2, 3, 4])])).unwrap();
    match anlz.sections.first().unwrap() {
        Section::Raw { tag, body } => {
            assert_eq!(*tag, FourCc::new(b"PZZZ"));
            assert_eq!(body, &[1, 2, 3, 4]);
        }
        other => panic!("expected Raw, got {other:?}"),
    }
}

#[test]
fn reads_waveform_stride_and_payload() {
    // PWV5: u4 entry size, u4 entry count, u4 unknown, then data.
    let mut body = 2_u32.to_be_bytes().to_vec();
    body.extend_from_slice(&3_u32.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    let anlz = parse(&build(&[(b"PWV5", body)])).unwrap();
    let (stride, data) = anlz.waveform(b"PWV5").unwrap();
    assert_eq!(*stride, 2);
    assert_eq!(data, &[1, 2, 3, 4, 5, 6]);
}

#[test]
fn a_section_longer_than_the_file_is_reported_not_read_past() {
    let mut file = build(&[(b"PQTZ", beat_grid(&[(1, 12800, 0)]))]);
    let len = file.len();
    // Overstate the section length.
    let section_len_at = 28 + 8;
    file[section_len_at..section_len_at + 4].copy_from_slice(&(len as u32 + 500).to_be_bytes());
    assert!(matches!(parse(&file), Err(AnlzError::BadSectionLength { .. })));
}

#[test]
fn a_zero_length_section_does_not_loop_forever() {
    let mut file = build(&[(b"PQTZ", beat_grid(&[(1, 12800, 0)]))]);
    let section_len_at = 28 + 8;
    file[section_len_at..section_len_at + 4].copy_from_slice(&0_u32.to_be_bytes());
    // Must return promptly with an error rather than spinning.
    assert!(parse(&file).is_err());
}

#[test]
fn several_sections_are_all_read() {
    let file = build(&[
        (b"PPTH", path_tag("/a.mp3")),
        (b"PQTZ", beat_grid(&[(1, 12000, 0)])),
        (b"PZZZ", vec![9]),
    ]);
    let anlz = parse(&file).unwrap();
    assert_eq!(anlz.sections.len(), 3);
    assert!(anlz.path().is_some());
    assert!(anlz.beat_grid().is_some());
}

#[test]
fn reading_a_missing_file_is_an_io_error() {
    let err = Anlz::read(std::path::Path::new("/definitely/not/here.DAT")).unwrap_err();
    assert!(matches!(err, AnlzError::Io(_)));
}

#[test]
fn resolve_joins_a_share_relative_path() {
    let share = std::path::Path::new("/Users/dj/Library/Pioneer/rekordbox/share");
    let p = rbl_anlz::resolve(share, "/PIONEER/USBANLZ/abc/def/ANLZ0000.DAT");
    assert!(p.ends_with("PIONEER/USBANLZ/abc/def/ANLZ0000.DAT"));
    assert!(p.starts_with(share));
}

#[test]
fn sibling_swaps_the_extension() {
    let dat = std::path::Path::new("/x/ANLZ0000.DAT");
    assert!(rbl_anlz::sibling(dat, "EXT").ends_with("ANLZ0000.EXT"));
    assert!(rbl_anlz::sibling(dat, "2EX").ends_with("ANLZ0000.2EX"));
}
