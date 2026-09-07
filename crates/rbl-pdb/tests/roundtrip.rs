//! Round-trips: what the builder writes, the reader must read back exactly.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_pdb::build::{device_sql_string, long_utf16le, short_ascii, FileBuilder, PageBuilder};
use rbl_pdb::{PageType, Pdb};

const PAGE: usize = 4096;

/// A genre row: u4 id then an inline string.
fn genre_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&device_sql_string(name));
    row
}

#[test]
fn a_built_file_parses_back() {
    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &[genre_row(1, "House"), genre_row(2, "Techno")]);
    let bytes = file.finish();

    let pdb = Pdb::parse(&bytes).unwrap();
    assert_eq!(pdb.page_size, 4096);
    let table = pdb.table(PageType::Genres).unwrap();
    let rows = pdb.named_rows(table);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, 1);
    assert_eq!(rows[0].name, "House");
    assert_eq!(rows[1].name, "Techno");
}

#[test]
fn many_rows_span_groups_and_pages() {
    // More than 16 forces multiple row groups; enough bytes forces a new page.
    let names: Vec<String> = (0..300).map(|i| format!("Genre number {i:03}")).collect();
    let rows: Vec<Vec<u8>> = names
        .iter()
        .enumerate()
        .map(|(i, n)| genre_row(u32::try_from(i).unwrap() + 1, n))
        .collect();

    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &rows);
    let bytes = file.finish();

    let pdb = Pdb::parse(&bytes).unwrap();
    let table = pdb.table(PageType::Genres).unwrap();
    let read = pdb.named_rows(table);
    assert_eq!(read.len(), 300, "every row must survive paging and grouping");
    assert_eq!(read[0].name, "Genre number 000");
    assert_eq!(read[299].name, "Genre number 299");
    assert_eq!(read[299].id, 300);
}

#[test]
fn short_ascii_length_is_mangled_the_way_the_format_expects() {
    // "incremented, doubled, and incremented again"
    let encoded = short_ascii("abc");
    assert_eq!(encoded[0], ((3 + 1) * 2 + 1) as u8);
    assert_eq!(&encoded[1..], b"abc");

    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &[genre_row(1, "abc")]);
    let bytes = file.finish();
    let pdb = Pdb::parse(&bytes).unwrap();
    assert_eq!(pdb.named_rows(pdb.table(PageType::Genres).unwrap())[0].name, "abc");
}

#[test]
fn utf16_strings_round_trip() {
    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &[genre_row(1, "Drum & Bass — Liquid ✨")]);
    let bytes = file.finish();
    let pdb = Pdb::parse(&bytes).unwrap();
    assert_eq!(
        pdb.named_rows(pdb.table(PageType::Genres).unwrap())[0].name,
        "Drum & Bass — Liquid ✨"
    );
}

#[test]
fn the_encoder_picks_utf16_only_when_it_must() {
    assert_eq!(device_sql_string("plain")[0] % 2, 1, "ASCII uses the short form");
    assert_eq!(device_sql_string("é")[0], 0x90, "non-ASCII uses UTF-16");
    assert_eq!(long_utf16le("")[0], 0x90);
}

#[test]
fn an_empty_string_reads_back_as_empty() {
    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &[genre_row(9, "")]);
    let bytes = file.finish();
    let pdb = Pdb::parse(&bytes).unwrap();
    let rows = pdb.named_rows(pdb.table(PageType::Genres).unwrap());
    assert_eq!(rows[0].id, 9);
    assert_eq!(rows[0].name, "");
}

#[test]
fn deleted_rows_are_skipped() {
    // Build a page by hand and clear one presence bit.
    let mut builder = PageBuilder::new(PAGE, 1, 1, 1);
    builder.push_row(&genre_row(1, "Kept"));
    builder.push_row(&genre_row(2, "Deleted"));
    builder.push_row(&genre_row(3, "AlsoKept"));
    let mut page = builder.finish();

    // Clear bit 1 in the first group's presence mask.
    let flags_at = PAGE - 4;
    let mut present = u16::from_le_bytes([page[flags_at], page[flags_at + 1]]);
    present &= !0b010;
    page[flags_at..flags_at + 2].copy_from_slice(&present.to_le_bytes());

    let mut file = vec![0_u8; PAGE];
    file[0x04..0x08].copy_from_slice(&(PAGE as u32).to_le_bytes());
    file[0x08..0x0c].copy_from_slice(&1_u32.to_le_bytes());
    file[28..32].copy_from_slice(&1_u32.to_le_bytes()); // page type genres
    file[36..40].copy_from_slice(&1_u32.to_le_bytes()); // first page
    file[40..44].copy_from_slice(&1_u32.to_le_bytes()); // last page
    file.extend_from_slice(&page);

    let pdb = Pdb::parse(&file).unwrap();
    let names: Vec<String> =
        pdb.named_rows(pdb.table(PageType::Genres).unwrap()).into_iter().map(|r| r.name).collect();
    assert_eq!(names, vec!["Kept".to_owned(), "AlsoKept".to_owned()]);
}

#[test]
fn rejects_files_that_are_not_devicesql() {
    assert!(Pdb::parse(&[]).is_err());
    assert!(Pdb::parse(&[0; 16]).is_err());
    // A plausible size but a nonsense page size.
    let mut junk = vec![0_u8; 4096];
    junk[4..8].copy_from_slice(&12345_u32.to_le_bytes());
    assert!(Pdb::parse(&junk).is_err());
}

#[test]
fn a_truncated_file_does_not_panic() {
    let mut file = FileBuilder::new(PAGE);
    file.add_table(1, &[genre_row(1, "House")]);
    let bytes = file.finish();
    for cut in [PAGE, PAGE + 100, bytes.len() - 1] {
        let short = &bytes[..cut.min(bytes.len())];
        if let Ok(pdb) = Pdb::parse(short) {
            // Must not panic; returning fewer rows is fine.
            let _ = pdb.census();
        }
    }
}

#[test]
fn a_self_referential_page_chain_terminates() {
    let mut file = vec![0_u8; PAGE * 2];
    file[0x04..0x08].copy_from_slice(&(PAGE as u32).to_le_bytes());
    file[0x08..0x0c].copy_from_slice(&1_u32.to_le_bytes());
    file[28..32].copy_from_slice(&1_u32.to_le_bytes());
    file[36..40].copy_from_slice(&1_u32.to_le_bytes());
    file[40..44].copy_from_slice(&999_u32.to_le_bytes()); // last page never reached
    // Page 1 points at itself.
    file[PAGE + 0x0c..PAGE + 0x10].copy_from_slice(&1_u32.to_le_bytes());
    let pdb = Pdb::parse(&file).unwrap();
    let _ = pdb.census(); // must return, not hang
}
