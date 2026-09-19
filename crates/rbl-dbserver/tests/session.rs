//! The menus a player is answered with, checked against rows rekordbox
//! 7.2.11 sent a CDJ-3000 (`docs/pre-release/verification/link/*-decoded.txt`).
//! Where a row depends on library data, a small catalog is built to hold
//! exactly what the captured row showed, so the bytes can be compared whole.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use rbl_dbserver::catalog::{Analysis, Catalog, Query, Row, Sort, TrackDetails, TrackScope};
use rbl_dbserver::item::TrackRow;
use rbl_dbserver::net::{Handler, Session};
use rbl_dbserver::session::CatalogHandler;
use rbl_dbserver::{kind, setup_request, Argument, Message};

/// The player's context word on its requests, as captured.
const CTX: u32 = 0x0101_0301;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// A library of one artist with one album and one track — the first rows
/// of the capture — plus the playlist folders it listed. The bool says
/// whether a player has loaded the track.
struct Small(bool);

const AALIYAH: u32 = 0x6d5c_28f2;
const ALBUM: u32 = 0xdc0d_0bca;
const TRACK: u32 = 0x475f;

fn the_track() -> TrackRow {
    TrackRow {
        id: TRACK,
        title: "At Your Best (You Are Love)".into(),
        comment: "Em - 156".into(),
        key: 0x14,
        key_name: "D".into(),
        artwork: 0x14,
        bpm_x100: 0x1e80,
    }
}

impl Catalog for Small {
    fn played(&self, track: u32) -> bool {
        self.0 && track == TRACK
    }
    fn list(&self, query: &Query) -> Vec<Row> {
        match query {
            Query::Artists(_) => vec![Row::Named { id: AALIYAH, name: "Aaliyah".into() }],
            Query::Albums(_) | Query::ArtistAlbums(_) => vec![Row::Named { id: ALBUM, name: "Aaliyah".into() }],
            Query::Folder(0) => vec![
                Row::List { id: 0xaa1f_785f, name: "CURRENT".into(), folder: true, position: 1 },
                Row::List { id: 0xffb7_d23b, name: "NP3-TEST-MP3".into(), folder: false, position: 0xb },
            ],
            Query::Histories => vec![Row::Named { id: 0x68ef_cd5c, name: "LINK HISTORY 2026-09-11".into() }],
            Query::Years => vec![Row::Date(2026), Row::Date(2025)],
            Query::Months(2026) => vec![Row::Date(1), Row::Date(2)],
            Query::Tracks { scope: TrackScope::Album(ALBUM), .. } => vec![Row::Track { id: TRACK, position: 0x55 }],
            Query::Tracks { .. } => vec![Row::Track { id: TRACK, position: 0 }],
            _ => vec![],
        }
    }
    fn track_row(&self, id: u32) -> Option<TrackRow> {
        (id == TRACK).then(the_track)
    }
    fn track(&self, id: u32) -> Option<TrackDetails> {
        (id == TRACK).then(|| TrackDetails {
            row: the_track(),
            artist_id: AALIYAH,
            artist: "Aaliyah".into(),
            duration_s: 0x122,
            genre_id: 0x8b0c_83f7,
            genre: "Pop".into(),
            date_added: "2023-08-06".into(),
            year: 0x140,
            bit_rate_kbps: 0x7ca,
            path: "/Volumes/SD/RB/Aaliyah/Unknown Album/70 at your best (you are love).mp3".into(),
            file_size: 0xb1_1858,
            file_type: 1,
            ..TrackDetails::default()
        })
    }
    fn artwork(&self, id: u32) -> Option<Vec<u8>> {
        (id == 0x6272).then(|| vec![0xff, 0xd8, 0xff, 0xe0])
    }
    fn analysis(&self, track: u32, what: &Analysis) -> Option<Vec<u8>> {
        match what {
            Analysis::Tag { fourcc, extension } if track == TRACK && fourcc == b"PWV4" && extension == b"EXT" => {
                Some(vec![b'P', b'W', b'V', b'4', 1, 2, 3])
            }
            _ => None,
        }
    }
}

fn session() -> Box<dyn Session> {
    session_with(Small(false))
}

fn session_with(catalog: Small) -> Box<dyn Session> {
    let handler = CatalogHandler::new(Arc::new(catalog));
    let mut session = handler.open();
    session.handle(&setup_request(1));
    session
}

fn numbers(kind: u16, tx: u32, args: &[u32]) -> Message {
    Message::new(tx, kind, args.iter().map(|&n| Argument::Number(n)).collect())
}

/// Asks for a menu and renders all of it; returns the items.
fn browse(session: &mut Box<dyn Session>, kind: u16, args: &[u32]) -> (u32, Vec<Message>) {
    let header = session.handle(&numbers(kind, 0x100, args));
    assert_eq!(header.len(), 1);
    assert_eq!(header[0].kind, rbl_dbserver::kind::MENU_HEADER);
    assert_eq!(header[0].arguments[0], Argument::Number(u32::from(kind)));
    let Argument::Number(count) = header[0].arguments[1] else { panic!() };
    let rendered = session.handle(&numbers(rbl_dbserver::kind::RENDER, 0x101, &[CTX, 0, count, 0, count, 0xc, 1, 0]));
    assert_eq!(rendered[0], Message::new(0x101, rbl_dbserver::kind::RENDER_HEADER, vec![Argument::Number(1), Argument::Number(0)]));
    assert_eq!(rendered.last().unwrap().kind, rbl_dbserver::kind::MENU_FOOTER);
    (count, rendered[1..rendered.len() - 1].to_vec())
}

fn args(item: &Message) -> String {
    item.arguments
        .iter()
        .map(|a| match a {
            Argument::Number(n) => format!("{n:#x}"),
            Argument::String(s) => format!("{s:?}"),
            Argument::Blob(b) => format!("blob[{}]", b.len()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn the_root_menu_is_rekordboxs_nine_categories_byte_for_byte() {
    let mut s = session();
    let (count, items) = browse(&mut s, kind::ROOT_MENU, &[CTX, 0, 0x5cf_ffff]);
    assert_eq!(count, 9);
    // The first row exactly as captured (transaction 0x180 in the capture).
    let artist = items[0].clone();
    let mut captured = artist.clone();
    captured.transaction = 0x180;
    assert_eq!(
        captured.encode(),
        hex("11872349ae11000001801041010f101400000010060606020602060606060606060602061100000000110000000211000000122600000009fffa004100520054004900530054fffb000011000000022600000001000011000000811100000000110000000011000000001100000000110000000011000000001100000002260000000100001100000000")
    );
    let labels: Vec<String> = items.iter().map(args).collect();
    assert_eq!(labels[3], "0x0, 0xc, 0xc, \"\\u{fffa}KEY\\u{fffb}\", 0x2, \"\", 0x8b, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(labels[8], "0x0, 0x1b, 0x1a, \"\\u{fffa}DATE ADDED\\u{fffb}\", 0x2, \"\", 0x8c, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

#[test]
fn the_sort_menu_matches_the_capture() {
    let mut s = session();
    let (count, items) = browse(&mut s, kind::SORT_MENU, &[0x0105_0301, 0, 0]);
    assert_eq!(count, 7);
    assert_eq!(args(&items[0]), "0x0, 0x0, 0x14, \"\\u{fffa}DEFAULT\\u{fffb}\", 0x2, \"\", 0xa1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[6]), "0x0, 0xc, 0xc, \"\\u{fffa}KEY\\u{fffb}\", 0x2, \"\", 0x8b, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

#[test]
fn the_key_menus_match_the_capture() {
    let mut s = session();
    let (count, items) = browse(&mut s, kind::KEY_MENU, &[CTX, 0]);
    assert_eq!(count, 24);
    assert_eq!(args(&items[0]), "0x0, 0x1, 0x8, \"Abm\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[23]), "0x0, 0x18, 0x4, \"E\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    let (count, items) = browse(&mut s, kind::RELATED_KEYS, &[0x0102_0301, 0, 1]);
    assert_eq!(count, 3);
    assert_eq!(args(&items[0]), "0x0, 0x1, 0x8, \"Abm\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[1]), "0x1, 0x1, 0xe, \"Abm, B\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[2]), "0x2, 0x1, 0x22, \"Abm, B, Dbm, Ebm\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

#[test]
fn artists_albums_and_their_tracks_are_shaped_as_captured() {
    let mut s = session();
    let (_, items) = browse(&mut s, kind::ARTIST_MENU, &[CTX, 0]);
    assert_eq!(args(&items[0]), "0x0, 0x6d5c28f2, 0x10, \"Aaliyah\", 0x2, \"\", 0x7, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");

    let (count, items) = browse(&mut s, kind::ARTIST_ALBUMS, &[CTX, 0, AALIYAH]);
    assert_eq!(count, 2, "⟨ALL⟩ then the album");
    assert_eq!(args(&items[0]), "0x0, 0xffffffff, 0xc, \"\\u{fffa}ALL\\u{fffb}\", 0x2, \"\", 0xa0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[1]), "0x0, 0xdc0d0bca, 0x10, \"Aaliyah\", 0x2, \"\", 0x2, 0x0, 0xdc0d0bca, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");

    // An artist's tracks carry flag 0x1000000; the album's the track number.
    let (_, items) = browse(&mut s, kind::ARTIST_ALBUM_TRACKS, &[CTX, 0, AALIYAH, 0xffff_ffff]);
    assert_eq!(args(&items[0]), "0x475f, 0x475f, 0x38, \"At Your Best (You Are Love)\", 0x12, \"Em - 156\", 0x2304, 0x1000000, 0x475f, 0x0, 0x100, 0x14, 0x14, 0x4, \"D\", 0x1e80");
    let (_, items) = browse(&mut s, kind::ALBUM_TRACKS, &[CTX, 0, ALBUM]);
    assert_eq!(args(&items[0]), "0x475f, 0x475f, 0x38, \"At Your Best (You Are Love)\", 0x12, \"Em - 156\", 0x2304, 0x1000000, 0x475f, 0x55, 0x100, 0x14, 0x14, 0x4, \"D\", 0x1e80");
    // In TRACK the flags are 0.
    let (_, items) = browse(&mut s, kind::TRACK_MENU, &[CTX, 0]);
    assert_eq!(args(&items[0]), "0x475f, 0x475f, 0x38, \"At Your Best (You Are Love)\", 0x12, \"Em - 156\", 0x2304, 0x0, 0x475f, 0x0, 0x100, 0x14, 0x14, 0x4, \"D\", 0x1e80");
}

#[test]
fn playlists_histories_and_dates_are_shaped_as_captured() {
    let mut s = session();
    let (_, items) = browse(&mut s, kind::PLAYLIST_MENU, &[CTX, 0, 0, 1]);
    assert_eq!(args(&items[0]), "0x0, 0xaa1f785f, 0x10, \"CURRENT\", 0x2, \"\", 0x1, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[1]), "0x0, 0xffb7d23b, 0x1a, \"NP3-TEST-MP3\", 0x2, \"\", 0x8, 0x0, 0x0, 0xb, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    // A playlist's tracks: flag 0x1000000 and the play order.
    let (_, items) = browse(&mut s, kind::PLAYLIST_MENU, &[CTX, 0, 0xffb7_d23b, 0]);
    assert!(args(&items[0]).contains("0x2304, 0x1000000, 0x475f, 0x0, 0x100"));
    // A history's tracks are all played.
    let (_, items) = browse(&mut s, kind::HISTORY_TRACKS, &[CTX, 0, 0x68ef_cd5c]);
    assert!(args(&items[0]).contains("0x2304, 0x100, 0x475f, 0x0, 0x100"));

    let (_, items) = browse(&mut s, kind::HISTORY_MENU, &[CTX, 0]);
    assert_eq!(args(&items[0]), "0x0, 0x68efcd5c, 0x30, \"LINK HISTORY 2026-09-11\", 0x2, \"\", 0x24, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");

    let (_, items) = browse(&mut s, kind::YEARS, &[CTX, 0]);
    assert_eq!(args(&items[0]), "0x0, 0x7ea, 0x2, \"\", 0x2, \"\", 0x2e, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    let (count, items) = browse(&mut s, kind::MONTHS, &[CTX, 0, 2026]);
    assert_eq!(count, 3);
    assert_eq!(args(&items[0]), "0x0, 0xffffffff, 0xc, \"\\u{fffa}ALL\\u{fffb}\", 0x2, \"\", 0xa0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(args(&items[1]), "0x0, 0x1, 0x2, \"\", 0x2, \"\", 0x2e, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

#[test]
fn metadata_and_track_info_have_the_captured_rows() {
    let mut s = session();
    let (count, items) = browse(&mut s, kind::METADATA, &[0x0102_0301, TRACK]);
    assert_eq!(count, 16);
    let rows: Vec<String> = items.iter().map(args).collect();
    assert_eq!(rows[0], "0x475f, 0x475f, 0x38, \"At Your Best (You Are Love)\", 0x12, \"Em - 156\", 0x2304, 0x0, 0x475f, 0x0, 0x100, 0x14, 0x14, 0x4, \"D\", 0x1e80");
    assert_eq!(rows[1], "0x1, 0x6d5c28f2, 0x10, \"Aaliyah\", 0x2, \"\", 0x7, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[2], "0x1, 0x0, 0x2, \"\", 0x2, \"\", 0x2, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[3], "0x0, 0x122, 0x2, \"\", 0x2, \"\", 0xb, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[4], "0x0, 0x1e80, 0x2, \"\", 0x2, \"\", 0xd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[5], "0x1, 0x14, 0x4, \"D\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[8], "0x0, 0x8b0c83f7, 0x8, \"Pop\", 0x2, \"\", 0x6, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[9], "0x1, 0x475f, 0x16, \"2023-08-06\", 0x2, \"\", 0x2e, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[10], "0x0, 0x475f, 0x12, \"Em - 156\", 0x2, \"\", 0x23, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[11], "0x0, 0x140, 0x2, \"\", 0x2, \"\", 0x10, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[12], "0x0, 0x7ca, 0x2, \"\", 0x2, \"\", 0x11, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[15], "0x0, 0x0, 0x2, \"\", 0x2, \"\", 0x29, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");

    let (count, items) = browse(&mut s, kind::TRACK_INFO, &[0x0108_0301, TRACK]);
    assert_eq!(count, 7);
    let rows: Vec<String> = items.iter().map(args).collect();
    assert_eq!(rows[4], "0xb11858, 0x475f, 0x90, \"/Volumes/SD/RB/Aaliyah/Unknown Album/70 at your best (you are love).mp3\", 0x2, \"\", 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[5], "0x0, 0x1, 0x2, \"\", 0x2, \"\", 0x2f, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[6], "0x0, 0x14, 0x4, \"D\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

/// After loading a track from a rekordbox source a CDJ-3000 asks for the KUVO
/// user info (`3006`) and waits on it: with no reply its next list request is
/// never sent ("Waiting…" after BACK, an 18 s timeout, two retries, the
/// source dropped). rekordbox 7.2.11 answers `4d02 [3006, 0, 160, blob]` and
/// the deck then asks for the delivery info (`2602`), a 13-row menu
/// (verification/link/kuvo-delivery-20260919.txt, frames 1407-1413).
#[test]
fn the_kuvo_user_info_and_delivery_info_are_answered_as_captured() {
    let mut s = session();
    let user = s.handle(&numbers(0x3006, 0xa2, &[0x0309_0301]));
    assert_eq!(user.len(), 1);
    // rekordbox's header, byte for byte; its blob carried its own account's
    // details (000482820000014d80042428…), ours is 160 zeros.
    let encoded = user[0].encode();
    let header = hex("11872349ae11000000a2104d020f041400000004060606031100003006110000000011000000a014000000a0");
    assert_eq!(&encoded[..header.len()], &header[..]);
    assert_eq!(encoded.len(), header.len() + 160);
    assert!(encoded[header.len()..].iter().all(|&b| b == 0));

    let (count, items) = browse(&mut s, 0x2602, &[0x0309_0301, TRACK]);
    assert_eq!(count, 13);
    let rows: Vec<String> = items.iter().map(args).collect();
    // The captured rows, with the capture's track ("Breaks 2", 0x18e460e, a
    // 13 s WAV at 140 BPM with no key, comment or label) read as ours.
    assert_eq!(rows[0], "0x0, 0x0, 0x2, \"\", 0x2, \"\", 0x36, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[1], "0x0, 0x6d5c28f2, 0x10, \"Aaliyah\", 0x2, \"\", 0x7, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[2], "0x0, 0x14, 0x2, \"\", 0x2, \"\", 0xf, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[3], "0x0, 0x122, 0x2, \"\", 0x2, \"\", 0xb, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[4], "0x475f, 0x475f, 0x38, \"At Your Best (You Are Love)\", 0x2, \"\", 0x4, 0x1000000, 0x475f, 0x0, 0x100, 0x0, 0x0, 0x2, \"\", 0x1e80");
    assert_eq!(rows[5], "0x0, 0x475f, 0x12, \"Em - 156\", 0x2, \"\", 0x23, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[6], "0x0, 0x0, 0x2, \"\", 0x2, \"\", 0x2, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[7], "0x0, 0x1e80, 0x2, \"\", 0x2, \"\", 0xd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[8], "0x0, 0x475f, 0x2, \"\", 0x2, \"\", 0x37, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[9], "0x0, 0x0, 0x2, \"\", 0x2, \"\", 0xe, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[10], "0x0, 0x1, 0x2, \"\", 0x2, \"\", 0x12, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[11], "0x0, 0x8b0c83f7, 0x8, \"Pop\", 0x2, \"\", 0x6, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
    assert_eq!(rows[12], "0x475f, 0x0, 0x2, \"\", 0x2, \"\", 0x4f, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x2, \"\", 0x0");
}

/// In the capture the track a player had loaded carried bit 0x100 in every
/// list and in its metadata; the tracks it had not, did not. A list that set
/// it on every row greyed the whole playlist on a CDJ.
#[test]
fn a_played_track_carries_the_played_bit_everywhere() {
    let mut s = session_with(Small(true));
    let (_, items) = browse(&mut s, kind::ARTIST_ALBUM_TRACKS, &[CTX, 0, AALIYAH, 0xffff_ffff]);
    assert!(args(&items[0]).contains("0x2304, 0x1000100, 0x475f, 0x0, 0x100"));
    let (_, items) = browse(&mut s, kind::PLAYLIST_MENU, &[CTX, 0, 0xffb7_d23b, 0]);
    assert!(args(&items[0]).contains("0x2304, 0x1000100, 0x475f, 0x0, 0x100"));
    let (_, items) = browse(&mut s, kind::TRACK_MENU, &[CTX, 0]);
    assert!(args(&items[0]).contains("0x2304, 0x100, 0x475f, 0x0, 0x100"));
    let (_, items) = browse(&mut s, kind::METADATA, &[0x0102_0301, TRACK]);
    assert!(args(&items[0]).contains("0x2304, 0x100, 0x475f, 0x0, 0x100"));
}

#[test]
fn artwork_and_tags_come_back_as_blobs_or_as_the_no_art_reply() {
    let mut s = session();
    let none = s.handle(&numbers(kind::ARTWORK, 0x187, &[0x0108_0301, 0xdc0d_0bca, 1]));
    assert_eq!(none[0].encode(), hex("11872349ae11000001871040020f04140000000406060603110000200311000000321100000000"));
    let some = s.handle(&numbers(kind::ARTWORK, 0x1a3, &[0x0108_0301, 0x6272, 1]));
    assert_eq!(args(&some[0]), "0x2003, 0x0, 0x4, blob[4]");

    let tag = s.handle(&numbers(kind::ANLZ_TAG, 0x197, &[0x0108_0301, TRACK, 0x3456_5750, 0x54_5845]));
    assert_eq!(tag[0].kind, kind::ANLZ_TAG_REPLY);
    assert_eq!(args(&tag[0]), "0x2c04, 0x0, 0x7, blob[7], 0x1");
    let missing = s.handle(&numbers(kind::ANLZ_TAG_2EX, 0x198, &[0x0108_0301, TRACK, 0x3656_5750, 0x58_4532]));
    assert_eq!(args(&missing[0]), "0x2d04, 0x32, 0x0, blob[0], 0x1");
    let grid = s.handle(&numbers(kind::BEAT_GRID, 0x199, &[0x0108_0301, TRACK]));
    assert_eq!(args(&grid[0]), "0x2204, 0x32, 0x0, blob[0], 0x0");
}

#[test]
fn a_page_of_a_long_list_is_the_window_asked_for() {
    struct Many;
    impl Catalog for Many {
        fn list(&self, _: &Query) -> Vec<Row> {
            (1..=100).map(|id| Row::Track { id, position: 0 }).collect()
        }
        fn track_row(&self, id: u32) -> Option<TrackRow> {
            Some(TrackRow { id, title: format!("Track {id}"), ..TrackRow::default() })
        }
        fn track(&self, _: u32) -> Option<TrackDetails> { None }
        fn artwork(&self, _: u32) -> Option<Vec<u8>> { None }
        fn analysis(&self, _: u32, _: &Analysis) -> Option<Vec<u8>> { None }
    }
    let handler = CatalogHandler::new(Arc::new(Many));
    let mut s = handler.open();
    let header = s.handle(&numbers(kind::TRACK_MENU, 1, &[CTX, 0]));
    assert_eq!(header[0].arguments[1], Argument::Number(100));
    // The deck pages 25 at a time: offset 50, limit 25.
    let page = s.handle(&numbers(kind::RENDER, 2, &[CTX, 50, 25, 0, 100, 0xc, 1, 0]));
    assert_eq!(page.len(), 27);
    assert_eq!(page[0].arguments, vec![Argument::Number(1), Argument::Number(50)]);
    assert_eq!(page[1].arguments[0], Argument::Number(51));
    assert_eq!(page[25].arguments[0], Argument::Number(75));
    // Beyond the end: header and footer only.
    let past = s.handle(&numbers(kind::RENDER, 3, &[CTX, 100, 25, 0, 100, 0xc, 1, 0]));
    assert_eq!(past.len(), 2);
}

#[test]
fn the_setup_reply_and_the_unknown_requests_answer_as_rekordbox_does() {
    let handler = CatalogHandler::new(Arc::new(Small(false)));
    let mut s = handler.open();
    let reply = s.handle(&setup_request(1));
    assert_eq!(reply[0].encode(), hex("11872349ae11fffffffe1000000f021400000002060611000000111100000014"));
    let after = s.handle(&numbers(0x3007, 0x17e, &[0x0108_0301, 0]));
    assert_eq!(after[0].encode(), hex("11872349ae110000017e1040000f021400000002060611000030071100000000"));
    let loaded = s.handle(&numbers(kind::LOADED, 0x18d, &[CTX, TRACK, 0, 1]));
    assert_eq!(loaded[0].encode(), hex("11872349ae110000018d1040000f021400000002060611000031001100000000"));
    let matching = s.handle(&numbers(0x1017, 0x3cf, &[0x0102_0301, 0, TRACK]));
    assert_eq!(args(&matching[0]), "0x1017, 0x0");
}

#[test]
fn tracks_are_sorted_the_way_the_player_asked() {
    // The sort id travels from the request into the query the catalog sees.
    struct Spy(std::sync::Mutex<Option<Query>>);
    impl Catalog for Spy {
        fn list(&self, q: &Query) -> Vec<Row> {
            *self.0.lock().unwrap() = Some(q.clone());
            vec![]
        }
        fn track_row(&self, _: u32) -> Option<TrackRow> { None }
        fn track(&self, _: u32) -> Option<TrackDetails> { None }
        fn artwork(&self, _: u32) -> Option<Vec<u8>> { None }
        fn analysis(&self, _: u32, _: &Analysis) -> Option<Vec<u8>> { None }
    }
    let spy = Arc::new(Spy(std::sync::Mutex::new(None)));
    let handler = CatalogHandler::new(Arc::clone(&spy) as Arc<dyn Catalog>);
    let mut s = handler.open();
    s.handle(&numbers(kind::TRACK_MENU, 1, &[CTX, 4]));
    assert_eq!(spy.0.lock().unwrap().clone(), Some(Query::Tracks { scope: TrackScope::All, sort: Sort::Bpm }));
    s.handle(&numbers(kind::KEY_TRACKS, 2, &[CTX, 0xc, 3, 2]));
    assert_eq!(spy.0.lock().unwrap().clone(), Some(Query::Tracks { scope: TrackScope::Key { key: 3, distance: 2 }, sort: Sort::Key }));
    s.handle(&Message::new(3, kind::SEARCH, vec![Argument::Number(CTX), Argument::Number(0), Argument::Number(8), Argument::String("ACID".into()), Argument::Number(0)]));
    assert_eq!(spy.0.lock().unwrap().clone(), Some(Query::Tracks { scope: TrackScope::Search("ACID".into()), sort: Sort::Default }));
}

#[test]
fn the_extended_cue_reply_counts_its_entries_not_a_header_word() {
    // The 2b04 blob is entries concatenated, each led by its own little-endian
    // byte length; the reply's trailing argument is how many there are. A CDJ
    // reads that count to size its cue table, so it must be the entry count,
    // not the word at a fixed offset — which is a cue's own fields (here a
    // deceptively large 0xffff) and once made a deck fault on 65 535 cues.
    struct Cued;
    impl Catalog for Cued {
        fn list(&self, _: &Query) -> Vec<Row> { Vec::new() }
        fn track_row(&self, _: u32) -> Option<TrackRow> { None }
        fn track(&self, _: u32) -> Option<TrackDetails> { None }
        fn artwork(&self, _: u32) -> Option<Vec<u8>> { None }
        fn analysis(&self, _: u32, what: &Analysis) -> Option<Vec<u8>> {
            match what {
                Analysis::ExtendedCueList => {
                    // Three 16-byte entries. Byte 4 of the first is 0xffff, the
                    // value the old code mistook for the count.
                    let mut blob = Vec::new();
                    for _ in 0..3 {
                        let mut e = vec![0_u8; 16];
                        e[0..4].copy_from_slice(&16_u32.to_le_bytes());
                        e[4..6].copy_from_slice(&0xffff_u16.to_le_bytes());
                        blob.extend_from_slice(&e);
                    }
                    Some(blob)
                }
                _ => None,
            }
        }
    }
    let handler = CatalogHandler::new(Arc::new(Cued));
    let mut s = handler.open();
    s.handle(&setup_request(1));
    let reply = s.handle(&numbers(kind::EXTENDED_CUES, 0x1c0, &[0x0108_0301, TRACK, 0]));
    assert_eq!(args(&reply[0]), "0x2b04, 0x0, 0x30, blob[48], 0x3");
}
