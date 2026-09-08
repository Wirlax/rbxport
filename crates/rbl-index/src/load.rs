//! Loads the whole library into the columnar index in one pass.
//!
//! Only live rows and only the columns the app actually reads: on the user's
//! library that is 38,681 of 115,613 content rows, so filtering in SQL rather
//! than in Rust is most of the win.

use std::collections::HashMap;
use std::time::Instant;

use rbl_db::Library as Db;
use rusqlite::Connection;

use crate::{strings::StrColumn, Library, Playlists, Row, NO_ID};

/// Converts a REAL to an integer without a lossy cast: NaN becomes 0 and
/// out-of-range values saturate.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "value is clamped into i64 range and NaN is handled explicitly"
)]
fn real_to_i64(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else {
        // `clamp` then round-trip through the integer range.
        let clamped = v.clamp(i64::MIN as f64, i64::MAX as f64);
        clamped.trunc() as i64
    }
}

/// Clamps into range before narrowing, so the conversion cannot truncate.
fn clamp_u32(v: i64) -> u32 {
    u32::try_from(v.clamp(0, i64::from(u32::MAX))).unwrap_or(0)
}
fn clamp_u16(v: i64) -> u16 {
    u16::try_from(v.clamp(0, i64::from(u16::MAX))).unwrap_or(0)
}
fn clamp_u8(v: i64, max: u8) -> u8 {
    u8::try_from(v.clamp(0, i64::from(max))).unwrap_or(0)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadStats {
    pub tracks: usize,
    pub playlists: usize,
    pub memberships: usize,
    pub read_ms: u128,
    pub index_ms: u128,
    pub heap_bytes: usize,
}

/// Reads a numeric column that rekordbox may store as INTEGER, REAL or TEXT.
///
/// Several nominally-numeric fields are TEXT in the real schema (`DBVersion`,
/// `ColorID`), so reading them as `i64` fails at runtime on some rows and not
/// others. Being tolerant here turns a class of crash into a `0`.
fn num(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<i64> {
    use rusqlite::types::ValueRef;
    #[allow(clippy::match_same_arms, reason = "each arm documents a distinct storage case")]
    Ok(match row.get_ref(idx)? {
        ValueRef::Integer(v) => v,
        ValueRef::Null => 0,
        // Saturating rather than a lossy `as`: no rekordbox field is this large,
        // and a NaN must not become an arbitrary integer.
        ValueRef::Real(v) => real_to_i64(v),
        ValueRef::Text(t) => std::str::from_utf8(t)
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map_or(0, real_to_i64),
        // A blob in a numeric column is meaningless; treat it as absent.
        ValueRef::Blob(_) => 0,
    })
}

/// Reads a lookup table into an interner, returning rekordbox id -> dense id.
fn load_lookup(
    conn: &Connection,
    table: &str,
    name_column: &str,
    interner: &mut crate::strings::Interner,
) -> rusqlite::Result<HashMap<String, u32>> {
    // Identifiers come from the constants below, never from user input.
    let sql = format!("SELECT ID, {name_column} FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let mut map = HashMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (Some(id), name) = row? else { continue };
        let dense = interner.push(name.as_deref().unwrap_or(""));
        map.insert(id, dense);
    }
    Ok(map)
}

/// Builds the index from an open (read-only is fine) library.
pub fn load(db: &Db) -> rusqlite::Result<(Library, LoadStats)> {
    let conn = db.connection();
    let t0 = Instant::now();
    let mut lib = Library::default();
    let mut stats = LoadStats::default();

    let artists = load_lookup(conn, "djmdArtist", "Name", &mut lib.artists)?;
    let albums = load_lookup(conn, "djmdAlbum", "Name", &mut lib.albums)?;
    let genres = load_lookup(conn, "djmdGenre", "Name", &mut lib.genres)?;
    let labels = load_lookup(conn, "djmdLabel", "Name", &mut lib.labels)?;
    let keys = load_lookup(conn, "djmdKey", "ScaleName", &mut lib.keys)?;

    let mut stmt = conn.prepare(
        "SELECT ID, Title, ArtistID, AlbumID, GenreID, LabelID, KeyID,
                BPM, Length, Rating, ColorID, FolderPath, FileNameL,
                AnalysisDataPath, DJPlayCount, StockDate, ReleaseDate, Commnt, Analysed
         FROM djmdContent
         WHERE rb_local_deleted = 0",
    )?;

    // Capacity guesses sized from the real library so the arenas rarely regrow.
    let expected = 40_000;
    lib.ids = Vec::with_capacity(expected);
    lib.title = StrColumn::with_capacity(expected, expected * 40);
    lib.title_folded = StrColumn::with_capacity(expected, expected * 40);
    lib.comment = StrColumn::with_capacity(expected, expected * 16);
    lib.folder_path = StrColumn::with_capacity(expected, expected * 90);
    lib.file_name = StrColumn::with_capacity(expected, expected * 40);
    lib.analysis_path = StrColumn::with_capacity(expected, expected * 60);
    lib.date_added = StrColumn::with_capacity(expected, expected * 11);
    lib.release_date = StrColumn::with_capacity(expected, expected * 11);

    let mut content_row: HashMap<u64, Row> = HashMap::with_capacity(expected);

    let lookup = |map: &HashMap<String, u32>, id: Option<String>| -> u32 {
        id.and_then(|k| map.get(&k).copied()).unwrap_or(NO_ID)
    };

    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let Some(id_text): Option<String> = r.get(0)? else { continue };
        let row_index = u32::try_from(lib.ids.len()).unwrap_or(u32::MAX);

        let title: String = r.get::<_, Option<String>>(1)?.unwrap_or_default();
        lib.title_folded.push(&crate::strings::fold(&title));
        lib.title.push(&title);

        lib.artist.push(lookup(&artists, r.get(2)?));
        lib.album.push(lookup(&albums, r.get(3)?));
        lib.genre.push(lookup(&genres, r.get(4)?));
        lib.label.push(lookup(&labels, r.get(5)?));
        lib.key.push(lookup(&keys, r.get(6)?));

        // BPM is stored x100; Length is whole seconds.
        lib.bpm_x100.push(clamp_u32(num(r, 7)?));
        lib.length_sec.push(clamp_u32(num(r, 8)?));
        lib.rating.push(clamp_u8(num(r, 9)?, 5));
        lib.color.push(clamp_u8(num(r, 10)?, u8::MAX));

        lib.folder_path.push(&r.get::<_, Option<String>>(11)?.unwrap_or_default());
        lib.file_name.push(&r.get::<_, Option<String>>(12)?.unwrap_or_default());
        lib.analysis_path.push(&r.get::<_, Option<String>>(13)?.unwrap_or_default());
        lib.play_count.push(clamp_u16(num(r, 14)?));
        lib.date_added.push(&r.get::<_, Option<String>>(15)?.unwrap_or_default());
        lib.release_date.push(&r.get::<_, Option<String>>(16)?.unwrap_or_default());
        lib.comment.push(&r.get::<_, Option<String>>(17)?.unwrap_or_default());
        // `Analysed` is a bitfield whose values are not yet all understood
        // (105/104/16/17/1 observed); non-zero means rekordbox analysed it.
        lib.analysed.push(u8::from(num(r, 18)? != 0));

        // Keyed by the parsed id, not the text: the map is only ever looked
        // up from a membership row, and parsing 75,386 of those is cheaper
        // than allocating 38,681 strings to key it by.
        let numeric_id = id_text.parse::<u64>().unwrap_or(0);
        content_row.insert(numeric_id, row_index);
        lib.ids.push(numeric_id);
    }
    lib.count = lib.ids.len();
    stats.tracks = lib.count;

    load_playlists(conn, &mut lib, &content_row, &mut stats)?;
    stats.read_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    lib.build_ranks();
    lib.build_search();
    stats.index_ms = t1.elapsed().as_millis();
    stats.heap_bytes = lib.heap_bytes();

    Ok((lib, stats))
}

fn load_playlists(
    conn: &Connection,
    lib: &mut Library,
    content_row: &HashMap<u64, Row>,
    stats: &mut LoadStats,
) -> rusqlite::Result<()> {
    let (playlists, memberships) = read_playlists(conn, content_row)?;
    stats.playlists = playlists.ids.len();
    stats.memberships = memberships;
    lib.set_playlists(playlists);
    Ok(())
}

/// Re-reads only the playlist tree, reusing the track columns already indexed.
///
/// A playlist edit changes nothing about the tracks, and re-reading everything
/// costs 233 ms against 24 ms for the playlist tables alone on the reference
/// library. Measured with `cargo run --release -p rbl-index --example
/// reload_split`.
pub fn reload_playlists(db: &Db, library: &Library) -> rusqlite::Result<Playlists> {
    // The content map is keyed by the id text, and the ids were parsed from
    // exactly that, so it rebuilds without touching the database.
    // Nothing is allocated here: the ids are already the map's keys.
    let mut content_row: HashMap<u64, Row> = HashMap::with_capacity(library.len());
    for (row, id) in library.ids.iter().enumerate() {
        content_row.insert(*id, u32::try_from(row).unwrap_or(u32::MAX));
    }
    let (playlists, _) = read_playlists(db.connection(), &content_row)?;
    Ok(playlists)
}

fn read_playlists(
    conn: &Connection,
    content_row: &HashMap<u64, Row>,
) -> rusqlite::Result<(Playlists, usize)> {
    let mut playlists = Playlists::default();
    let mut index_by_id: HashMap<String, usize> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT ID, Name, ParentID, Seq FROM djmdPlaylist
         WHERE rb_local_deleted = 0 ORDER BY Seq",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let Some(id): Option<String> = r.get(0)? else { continue };
        let numeric_id = id.parse::<u64>().unwrap_or(0);
        index_by_id.insert(id, playlists.ids.len()); // moved, not cloned
        playlists.ids.push(numeric_id);
        playlists.names.push(&r.get::<_, Option<String>>(1)?.unwrap_or_default());
        // Parent is resolved after every playlist is known.
        playlists.parent.push(NO_ID);
        playlists.seq.push(clamp_u32(num(r, 3)?));
        playlists.members.push(Vec::new());
    }

    // Second pass for parents, now that every id has an index.
    let mut stmt = conn.prepare(
        "SELECT ID, ParentID FROM djmdPlaylist WHERE rb_local_deleted = 0",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let id: Option<String> = r.get(0)?;
        let parent: Option<String> = r.get(1)?;
        let Some(id) = id else { continue };
        let (Some(&child), Some(parent_index)) = (
            index_by_id.get(&id),
            parent.as_ref().and_then(|p| index_by_id.get(p)).copied(),
        ) else {
            continue;
        };
        if let Some(slot) = playlists.parent.get_mut(child) {
            *slot = u32::try_from(parent_index).unwrap_or(NO_ID);
        }
    }

    let mut stmt = conn.prepare(
        "SELECT PlaylistID, ContentID FROM djmdSongPlaylist
         WHERE rb_local_deleted = 0 ORDER BY PlaylistID, TrackNo",
    )?;
    let mut rows = stmt.query([])?;
    let mut memberships = 0usize;
    while let Some(r) = rows.next()? {
        let (playlist_id, content_id): (Option<String>, Option<String>) = (r.get(0)?, r.get(1)?);
        let (Some(playlist_id), Some(content_id)) = (playlist_id, content_id) else { continue };
        let content_key = content_id.parse::<u64>().unwrap_or(0);
        let (Some(&pi), Some(&row)) = (index_by_id.get(&playlist_id), content_row.get(&content_key))
        else {
            continue; // membership pointing at a deleted track
        };
        if let Some(members) = playlists.members.get_mut(pi) {
            members.push(row);
            memberships += 1;
        }
    }

    Ok((playlists, memberships))
}
