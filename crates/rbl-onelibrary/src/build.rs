//! Writing an `exportLibrary.db`.
//!
//! Everything here — the schema, the menu definitions, the colour names — was
//! transcribed from a real rekordbox-authored export
//! (`~/code/cdj3k-emu/tests/fixtures/usb-real.img`), read with
//! `cargo run -p rbl-onelibrary --example inspect`. None of it is invented.
//!
//! The reference tables matter more than they look. `menuItem`, `category` and
//! `sort` are how rekordbox knows which browse columns exist and in what
//! order; an export without them opens but browses wrong.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::{key, unlock, Error, Result};

/// `dbVersion` written into `property`, as the reference export carries it.
pub const DB_VERSION: &str = "1000";

/// The schema, verbatim from a real export. Twenty-two tables, several of
/// which stay empty but must exist.
const SCHEMA: &[&str] = &[
    "CREATE TABLE album(album_id integer primary key, name varchar, artist_id integer, image_id integer, isComplation integer, nameForSearch varchar)",
    "CREATE TABLE artist(artist_id integer primary key, name varchar, nameForSearch varchar)",
    "CREATE TABLE category(category_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer)",
    "CREATE TABLE color(color_id integer primary key, name varchar)",
    "CREATE TABLE content(content_id integer primary key, title varchar, titleForSearch varchar, subtitle varchar, bpmx100 integer, length integer, trackNo integer, discNo integer, artist_id_artist integer, artist_id_remixer integer, artist_id_originalArtist integer, artist_id_composer integer, artist_id_lyricist integer, album_id integer, genre_id integer, label_id integer, key_id integer, color_id integer, image_id integer, djComment varchar, rating integer, releaseYear integer, releaseDate varchar, dateCreated varchar, dateAdded varchar, path varchar, fileName varchar, fileSize integer, fileType integer, bitrate integer, bitDepth integer, samplingRate integer, isrc varchar, djPlayCount integer, isHotCueAutoLoadOn integer, isKuvoDeliverStatusOn integer, kuvoDeliveryComment varchar, masterDbId integer, masterContentId integer, analysisDataFilePath varchar, analysedBits integer, contentLink integer, hasModified integer, cueUpdateCount integer, analysisDataUpdateCount integer, informationUpdateCount integer)",
    "CREATE TABLE cue(cue_id integer primary key, content_id integer, kind integer, colorTableIndex integer, cueComment varchar, isActiveLoop integer, beatLoopNumerator integer, beatLoopDenominator integer, inUsec integer, outUsec integer, in150FramePerSec integer, out150FramePerSec integer, inMpegFrameNumber integer, outMpegFrameNumber integer, inMpegAbs integer, outMpegAbs integer, inDecodingStartFramePosition integer, outDecodingStartFramePosition integer, inFileOffsetInBlock integer, OutFileOffsetInBlock integer, inNumberOfSampleInBlock integer, outNumberOfSampleInBlock integer)",
    "CREATE TABLE genre(genre_id integer primary key, name varchar)",
    "CREATE TABLE history(history_id integer primary key, sequenceNo integer, name varchar, attribute integer, history_id_parent integer)",
    "CREATE TABLE history_content(history_id integer, content_id integer, sequenceNo integer)",
    "CREATE TABLE hotCueBankList(hotCueBankList_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, hotCueBankList_id_parent integer)",
    "CREATE TABLE hotCueBankList_cue(hotCueBankList_id integer, cue_id integer, sequenceNo integer)",
    "CREATE TABLE image(image_id integer primary key, path varchar)",
    "CREATE TABLE key(key_id integer primary key, name varchar)",
    "CREATE TABLE label(label_id integer primary key, name varchar)",
    "CREATE TABLE menuItem(menuItem_id integer primary key, kind integer, name varchar)",
    "CREATE TABLE myTag(myTag_id integer primary key, sequenceNo integer, name varchar, attribute integer, myTag_id_parent integer)",
    "CREATE TABLE myTag_content(myTag_id integer, content_id integer)",
    "CREATE TABLE playlist(playlist_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, playlist_id_parent integer)",
    "CREATE TABLE playlist_content(playlist_id integer, content_id integer, sequenceNo integer)",
    "CREATE TABLE property(deviceName varchar, dbVersion varchar, numberOfContents integer, createdDate varchar, backGroundColorType integer, myTagMasterDBID integer)",
    "CREATE TABLE recommendedLike(content_id_1 integer, content_id_2 integer, rating integer, createdDate integer)",
    "CREATE TABLE sort(sort_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer, isSelectedAsSubColumn integer)",
];

/// Browse menu definitions.
///
/// The names are wrapped in U+FFFA and U+FFFB — interlinear annotation
/// markers, which is how rekordbox flags a string for translation at display
/// time. Writing the bare word instead leaves a player showing English
/// whatever its language is set to, so the wrapping is reproduced.
const MENU_ITEMS: &[(i64, i64, &str)] = &[
    (1, 128, "GENRE"),
    (2, 129, "ARTIST"),
    (3, 130, "ALBUM"),
    (4, 131, "TRACK"),
    (5, 133, "BPM"),
    (6, 134, "RATING"),
    (7, 135, "YEAR"),
    (8, 136, "REMIXER"),
    (9, 137, "LABEL"),
    (10, 138, "ORIGINAL ARTIST"),
    (11, 139, "KEY"),
    (12, 141, "CUE"),
    (13, 142, "COLOR"),
    (14, 146, "TIME"),
    (15, 147, "BITRATE"),
    (16, 148, "FILE NAME"),
    (17, 132, "PLAYLIST"),
    (18, 152, "HOT CUE BANK"),
    (19, 149, "HISTORY"),
    (20, 145, "SEARCH"),
    (21, 150, "COMMENTS"),
    (22, 140, "DATE ADDED"),
    (23, 151, "DJ PLAY COUNT"),
    (24, 144, "FOLDER"),
    (25, 161, "DEFAULT"),
    (26, 162, "ALPHABET"),
    (27, 170, "MATCHING"),
];

/// Which menu items appear as browse categories, and in what order.
const CATEGORIES: &[(i64, i64, i64, i64)] = &[
    (1, 1, 0, 0),
    (2, 2, 1, 1),
    (3, 3, 2, 1),
    (4, 4, 3, 1),
    (5, 17, 5, 1),
    (6, 5, 0, 0),
    (7, 6, 0, 0),
    (8, 7, 0, 0),
    (9, 8, 0, 0),
    (10, 9, 0, 0),
    (11, 10, 0, 0),
    (12, 11, 4, 1),
    (15, 13, 0, 0),
    (17, 24, 9, 1),
    (18, 20, 7, 1),
    (19, 14, 0, 0),
    (20, 15, 0, 0),
    (21, 16, 0, 0),
    (22, 19, 6, 1),
    (23, 18, 0, 0),
    (26, 27, 8, 1),
    (27, 22, 10, 1),
];

/// Which menu items appear as sort columns.
const SORTS: &[(i64, i64, i64, i64, i64)] = &[
    (0, 25, 1, 1, 0),
    (1, 26, 2, 1, 0),
    (2, 2, 3, 1, 0),
    (3, 3, 4, 1, 0),
    (4, 5, 5, 1, 0),
    (5, 6, 6, 1, 0),
    (6, 1, 0, 0, 0),
    (7, 21, 0, 0, 1),
    (8, 14, 0, 0, 0),
    (9, 8, 0, 0, 0),
    (10, 9, 0, 0, 0),
    (11, 10, 0, 0, 0),
    (12, 11, 7, 1, 0),
    (13, 15, 0, 0, 0),
    (15, 13, 0, 0, 0),
    (16, 23, 0, 0, 0),
    (17, 22, 0, 0, 0),
];

/// The eight colours in rekordbox's own order, so `color_id` lines up with the
/// `ColorID` stored against a track.
const COLORS: &[&str] = &[
    "Pink",
    "Red",
    "Orange",
    "Yellow",
    "Green",
    "Aqua",
    "Blue",
    "Purple",
];

/// Wraps a menu name in the annotation markers rekordbox uses.
fn annotated(name: &str) -> String {
    format!("\u{FFFA}{name}\u{FFFB}")
}

/// A track as `exportLibrary.db` stores it.
///
/// Ids are the stick-local ones an export assigns, not rekordbox's.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub content_id: i64,
    pub title: String,
    pub artist_id: Option<i64>,
    pub album_id: Option<i64>,
    pub genre_id: Option<i64>,
    pub label_id: Option<i64>,
    pub key_id: Option<i64>,
    pub color_id: Option<i64>,
    pub bpm_x100: i64,
    /// Playing time in seconds.
    pub length: i64,
    pub track_no: i64,
    /// Stick-relative, e.g. `/Contents/ARTBAT/The Abyss.mp3`.
    pub path: String,
    pub file_name: String,
    pub file_size: i64,
    /// Stick-relative path of the analysis file.
    pub analysis_path: String,
    /// 0 to 255, in the multiples of 51 rekordbox uses for stars.
    pub rating: i64,
    pub comment: String,
    pub date_added: String,
}

/// Builds an `exportLibrary.db`.
#[derive(Debug)]
pub struct Builder {
    conn: Connection,
    tracks: i64,
}

impl Builder {
    /// Creates a database at `path` with the schema and reference tables in
    /// place. Refuses to overwrite an existing file.
    pub fn create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Err(Error::Exists(path.display().to_string()));
        }
        let conn = Connection::open(path)
            .map_err(|source| Error::Open { path: path.display().to_string(), source })?;
        // The cipher settings must precede every other statement, or the file
        // is created as plain SQLite and no player can read it.
        unlock(&conn, &key::passphrase()?)?;

        for statement in SCHEMA {
            conn.execute(statement, [])?;
        }
        for (id, kind, name) in MENU_ITEMS {
            conn.execute(
                "INSERT INTO menuItem (menuItem_id, kind, name) VALUES (?1, ?2, ?3)",
                params![id, kind, annotated(name)],
            )?;
        }
        for (id, menu_item, seq, visible) in CATEGORIES {
            conn.execute(
                "INSERT INTO category (category_id, menuItem_id, sequenceNo, isVisible)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, menu_item, seq, visible],
            )?;
        }
        for (id, menu_item, seq, visible, sub_column) in SORTS {
            conn.execute(
                "INSERT INTO sort
                    (sort_id, menuItem_id, sequenceNo, isVisible, isSelectedAsSubColumn)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, menu_item, seq, visible, sub_column],
            )?;
        }
        for (index, name) in COLORS.iter().enumerate() {
            conn.execute(
                "INSERT INTO color (color_id, name) VALUES (?1, ?2)",
                params![i64::try_from(index + 1).unwrap_or(1), name],
            )?;
        }

        Ok(Self { conn, tracks: 0 })
    }

    /// Adds a lookup row and returns its id, reusing one that already matches.
    ///
    /// An empty name is id 0, which is how the reference export spells "none".
    pub fn intern(&mut self, table: LookupTable, name: &str) -> Result<i64> {
        if name.is_empty() {
            return Ok(0);
        }
        let (table, id_column) = (table.name(), table.id_column());
        let existing: Option<i64> = self
            .conn
            .query_row(
                &format!("SELECT {id_column} FROM {table} WHERE name = ?1"),
                params![name],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = existing {
            return Ok(id);
        }
        let next: i64 = self
            .conn
            .query_row(
                &format!("SELECT COALESCE(MAX({id_column}), 0) + 1 FROM {table}"),
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);
        if table == "artist" {
            // Artist is the only lookup the reference export fills a search
            // column for.
            self.conn.execute(
                "INSERT INTO artist (artist_id, name, nameForSearch) VALUES (?1, ?2, ?2)",
                params![next, name],
            )?;
        } else {
            self.conn.execute(
                &format!("INSERT INTO {table} ({id_column}, name) VALUES (?1, ?2)"),
                params![next, name],
            )?;
        }
        Ok(next)
    }

    /// Adds a track.
    pub fn add_track(&mut self, track: &Track) -> Result<()> {
        self.conn.execute(
            "INSERT INTO content
                (content_id, title, titleForSearch, bpmx100, length, trackNo,
                 artist_id_artist, album_id, genre_id, label_id, key_id, color_id,
                 djComment, rating, dateAdded, path, fileName, fileSize,
                 analysisDataFilePath, djPlayCount, hasModified)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                     ?12, ?13, ?14, ?15, ?16, ?17, ?18, 0, 0)",
            params![
                track.content_id,
                track.title,
                track.bpm_x100,
                track.length,
                track.track_no,
                track.artist_id,
                track.album_id,
                track.genre_id,
                track.label_id,
                track.key_id,
                track.color_id,
                track.comment,
                track.rating,
                track.date_added,
                track.path,
                track.file_name,
                track.file_size,
                track.analysis_path,
            ],
        )?;
        self.tracks += 1;
        Ok(())
    }

    /// Adds a playlist.
    ///
    /// `parent` is 0 for the top level — a number, unlike `master.db`, which
    /// spells the same thing as the string `"root"`.
    pub fn add_playlist(&mut self, id: i64, name: &str, parent: i64, seq: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO playlist
                (playlist_id, sequenceNo, name, image_id, attribute, playlist_id_parent)
             VALUES (?1, ?2, ?3, NULL, 0, ?4)",
            params![id, seq, name, parent],
        )?;
        Ok(())
    }

    /// Places a track in a playlist. `seq` is one-based.
    pub fn add_to_playlist(&mut self, playlist: i64, content: i64, seq: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO playlist_content (playlist_id, content_id, sequenceNo)
             VALUES (?1, ?2, ?3)",
            params![playlist, content, seq],
        )?;
        Ok(())
    }

    /// Writes the property row and closes the database.
    ///
    /// `created` is a date, `YYYY-MM-DD`, which is what the reference export
    /// carries — not a full timestamp.
    pub fn finish(self, device_name: &str, created: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO property
                (deviceName, dbVersion, numberOfContents, createdDate,
                 backGroundColorType, myTagMasterDBID)
             VALUES (?1, ?2, ?3, ?4, 0, 0)",
            params![device_name, DB_VERSION, self.tracks, created],
        )?;
        // A stick must not be left with pages only in the WAL: a device that
        // does not replay it would read a database missing everything written.
        self.conn.pragma_update(None, "journal_mode", "DELETE")?;
        drop(self.conn);
        Ok(())
    }
}

/// The lookup tables a track refers to.
///
/// An enum rather than a string, so a table name can never reach the SQL from
/// outside this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupTable {
    Artist,
    Album,
    Genre,
    Label,
    Key,
}

impl LookupTable {
    const fn name(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Genre => "genre",
            Self::Label => "label",
            Self::Key => "key",
        }
    }

    const fn id_column(self) -> &'static str {
        match self {
            Self::Artist => "artist_id",
            Self::Album => "album_id",
            Self::Genre => "genre_id",
            Self::Label => "label_id",
            Self::Key => "key_id",
        }
    }
}
