//! Guarded writes to the library.
//!
//! # What the shapes are based on
//!
//! Every column value here was read off the user's own library rather than
//! guessed, using `cargo run -p rbl-db --example row_shapes`. The population
//! statistics matter more than any single row:
//!
//! - **A locally-created row has `rb_data_status = 0`.** All 57 playlists and
//!   all 56,376 playlist memberships with `usn IS NULL` — i.e. made on this
//!   machine and never synced — carry 0. The values 256/257/258 seen elsewhere
//!   are written by the cloud sync, not by creation, and 257 is *not* the
//!   folder marker it looks like from two samples: it appears under both
//!   `Attribute` values.
//! - **`usn` stays NULL** until sync assigns one; `rb_local_usn` is ours.
//! - **A soft delete sets `rb_local_deleted = 1` and nothing else** —
//!   `rb_data_status` is unchanged on all 919 deleted rows.
//! - **`TrackNo` is contiguous from 1** in every one of the 683 playlists.
//! - **`Seq` is not**: parents start at 0 or 1, so a new entry appends at
//!   `MAX(Seq) + 1` rather than assuming a base.
//! - **Timestamps are UTC with an explicit `+00:00`**, 1,575 of 1,602.
//!
//! # Cues
//!
//! Cue writing was blocked on three unexplained fields. Counting the
//! reference library's 1,040,598 cues settled all three:
//!
//! - **`ColorTableIndex` is not a per-slot palette.** Index 21 dominates every
//!   hot-cue kind alike — 169,389 of kind 1, 171,149 of kind 2 — so it is the
//!   default colour, not a slot's own. Memory cues carry 0 or NULL.
//! - **`Color`** is 255 on memory cues and -1 on hot ones.
//! - **`BeatLoopSize`** is NULL or 0 on every one of the 1,040,176 cues that
//!   is not a loop; only the 422 loops set it.
//!
//! **`BeatLoopSize` encodes the loop's length in beats** as
//! `(beats << 16) | 1`. Every value in the library fits: 65537, 524289,
//! 1048577, 2097153 and 4194305 are 1, 8, 16, 32 and 64 beats. The one loop
//! whose track is still live carries 262145 — four beats — and its In/Out span
//! measures exactly four beats at the track's own BPM. Zero means the length
//! is implied by In/Out rather than stated.
//!
//! So a plain cue and a loop are both determined. Setting a *custom* colour
//! still is not — what RGB an index past the default means is unknown — so
//! that alone is refused.
//!
//! # What this deliberately will not do
//!
//! Analysis registration (`Analysed`, `AnalysisUpdated`), loop cues, custom
//! cue colours, and `contentCue`/`contentFile` are **not implemented**. Their
//! values are still unexplained, and a wrong one in a 38,681-track collection
//! is not recoverable by undo. See [`Unsupported`].

use std::path::{Path, PathBuf};

use rbl_core::ids::{Rng, MAX_CONTENT_ID, MAX_PLAYLIST_ID};
use rbl_core::time;
use rusqlite::types::Value;
use rusqlite::{params, Connection, TransactionBehavior};

use crate::{is_rekordbox_running, DbError, Library, OpenMode, Result};

/// Tables whose `rb_local_usn` participates in the shared counter.
const USN_TABLES: &[&str] = &["djmdContent", "djmdPlaylist", "djmdSongPlaylist"];

/// `djmdPlaylist.Attribute`: an ordinary playlist.
pub const ATTRIBUTE_PLAYLIST: i64 = 0;
/// `djmdPlaylist.Attribute`: a folder that holds other playlists.
pub const ATTRIBUTE_FOLDER: i64 = 1;

/// `ParentID` of a top-level playlist or folder. A string, not an id.
pub const ROOT: &str = "root";

/// How many backups of the library to keep.
const BACKUPS_KEPT: usize = 5;

/// Attempts before giving up on finding an unused id.
const ID_ATTEMPTS: usize = 64;

/// Columns [`Writer::touch`] will set. A column name is interpolated into SQL,
/// so the set of legal names is spelled out rather than trusted.
const WRITABLE_COLUMNS: &[&str] = &["Name", "Rating", "Commnt", "ColorID", "FolderPath", "FileNameL"];

/// Things the writer refuses to do, and why.
///
/// Returned as an error rather than silently skipped, so a caller cannot
/// believe an unsupported edit succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unsupported {
    /// Whether `Analysed`'s bits track which tags a track carries was tested
    /// and is unconfirmed — see `analysed_bits`. What the test does establish
    /// is enough to refuse on: 105, the value on 37,652 of 38,681 tracks,
    /// comes with PSSI, and our phrase detector produces none.
    AnalysisRegistration,
    /// What RGB a `ColorTableIndex` past the default means is unknown.
    CueColour,
    /// Nothing is known about what rekordbox does with these.
    ContentCueOrFile,
}

impl Unsupported {
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::AnalysisRegistration =>
                "registering analysis needs the Analysed bitfield explained by a diff recording",
            Self::CueColour =>
                "setting a cue's colour needs the ColorTableIndex palette explained by a diff recording",
            Self::ContentCueOrFile =>
                "contentCue and contentFile are not understood and must not be touched",
        }
    }
}

/// A guarded write session.
///
/// Holds the library open read-write. Every action is one immediate
/// transaction, and the process gate is re-checked before each: rekordbox can
/// be launched at any moment, and a check made when the session opened would
/// be stale by the time an edit happens.
#[derive(Debug)]
pub struct Writer {
    library: Library,
    rng: Rng,
    backup_dir: PathBuf,
    backup_taken: bool,
}

/// What one action changed. Returned so a caller can report it and a test can
/// assert on it without re-querying.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changed {
    pub rows: usize,
    /// The USN assigned to the last row written.
    pub usn: i64,
}

impl Writer {
    /// Opens the library for writing and prepares the backup directory.
    ///
    /// Fails if rekordbox is running, or if this is the real library under
    /// `RB_LITE_TEST` — both enforced by [`Library::open`].
    pub fn open(location: crate::LibraryLocation, backup_dir: impl Into<PathBuf>) -> Result<Self> {
        let library = Library::open(location, OpenMode::ReadWrite)?;
        if let crate::SchemaSupport::Degraded { missing } = &library.schema().support {
            return Err(DbError::WriteRefused(format!(
                "the schema is missing {}; writes are disabled rather than guessed",
                missing.join(", ")
            )));
        }
        Ok(Self {
            library,
            rng: Rng::from_entropy(),
            backup_dir: backup_dir.into(),
            backup_taken: false,
        })
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Every unsupported edit, refused with its reason.
    pub fn refuse(action: Unsupported) -> DbError {
        DbError::WriteRefused(action.reason().to_owned())
    }

    // ------------------------------------------------------------- playlists

    /// Creates a playlist and returns its id.
    pub fn create_playlist(&mut self, name: &str, parent: &str) -> Result<String> {
        self.create_node(name, parent, ATTRIBUTE_PLAYLIST)
    }

    /// Creates a folder and returns its id.
    pub fn create_folder(&mut self, name: &str, parent: &str) -> Result<String> {
        self.create_node(name, parent, ATTRIBUTE_FOLDER)
    }

    fn create_node(&mut self, name: &str, parent: &str, attribute: i64) -> Result<String> {
        self.prepare()?;
        let id = self.unused_id("djmdPlaylist")?;
        let uuid = self.rng.uuid4();
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if parent != ROOT && !node_exists(&tx, parent)? {
            return Err(DbError::WriteRefused(format!("no playlist or folder {parent}")));
        }
        // Seq appends: parents in the reference library start at 0 or 1, so
        // there is no base to assume.
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(Seq), -1) + 1 FROM djmdPlaylist
             WHERE ParentID = ?1 AND rb_local_deleted = 0",
            params![parent],
            |r| r.get(0),
        )?;
        let usn = next_usn(&tx);
        tx.execute(
            "INSERT INTO djmdPlaylist
                (ID, Seq, Name, ImagePath, Attribute, ParentID, SmartList, UUID,
                 rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced,
                 usn, rb_local_usn, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, NULL, ?6, 0, 0, 0, 0, NULL, ?7, ?8, ?8)",
            params![id, seq, name, attribute, parent, uuid, usn, stamp],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(id)
    }

    /// Renames a playlist or folder.
    pub fn rename(&mut self, id: &str, name: &str) -> Result<Changed> {
        self.touch_playlist(id, "Name", &Value::Text(name.to_owned()))
    }

    /// Moves a playlist or folder under a new parent, appended at the end.
    pub fn move_to(&mut self, id: &str, parent: &str) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if parent != ROOT && !node_exists(&tx, parent)? {
            return Err(DbError::WriteRefused(format!("no playlist or folder {parent}")));
        }
        if parent == id || is_descendant(&tx, parent, id) {
            // Reparenting a folder under itself detaches the whole subtree from
            // the tree and it is never seen again.
            return Err(DbError::WriteRefused(
                "that would put a folder inside itself".to_owned(),
            ));
        }
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(Seq), -1) + 1 FROM djmdPlaylist
             WHERE ParentID = ?1 AND rb_local_deleted = 0",
            params![parent],
            |r| r.get(0),
        )?;
        let usn = next_usn(&tx);
        let rows = tx.execute(
            "UPDATE djmdPlaylist SET ParentID = ?1, Seq = ?2, rb_local_usn = ?3, updated_at = ?4
             WHERE ID = ?5 AND rb_local_deleted = 0",
            params![parent, seq, usn, stamp, id],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    /// Soft-deletes a playlist or folder, everything inside it, and every
    /// membership that pointed at it.
    ///
    /// Never `DELETE`: rekordbox's own sync relies on the tombstone.
    pub fn delete_playlist(&mut self, id: &str) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Collect the subtree first: deleting as we walk would hide children
        // from the walk.
        let mut doomed = vec![id.to_owned()];
        let mut frontier = vec![id.to_owned()];
        while let Some(parent) = frontier.pop() {
            let mut stmt = tx.prepare(
                "SELECT ID FROM djmdPlaylist WHERE ParentID = ?1 AND rb_local_deleted = 0",
            )?;
            let children: Vec<String> = stmt
                .query_map(params![parent], |r| r.get::<_, String>(0))?
                .filter_map(std::result::Result::ok)
                .collect();
            for child in children {
                doomed.push(child.clone());
                frontier.push(child);
            }
        }

        let mut rows = 0;
        let mut usn = 0;
        for node in &doomed {
            usn = next_usn(&tx);
            rows += tx.execute(
                "UPDATE djmdSongPlaylist SET rb_local_deleted = 1, rb_local_usn = ?1,
                    updated_at = ?2 WHERE PlaylistID = ?3 AND rb_local_deleted = 0",
                params![usn, stamp, node],
            )?;
            usn = next_usn(&tx);
            rows += tx.execute(
                "UPDATE djmdPlaylist SET rb_local_deleted = 1, rb_local_usn = ?1,
                    updated_at = ?2 WHERE ID = ?3 AND rb_local_deleted = 0",
                params![usn, stamp, node],
            )?;
        }
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    // ------------------------------------------------------------ membership

    /// Appends tracks to a playlist, skipping any already in it.
    ///
    /// `TrackNo` stays contiguous from 1, which is true of every playlist in
    /// the reference library.
    pub fn add_tracks(&mut self, playlist: &str, contents: &[String]) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let mut ids: Vec<(String, String)> = Vec::with_capacity(contents.len());
        for _ in contents {
            ids.push((self.rng.uuid4(), self.rng.uuid4()));
        }

        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !node_exists(&tx, playlist)? {
            return Err(DbError::WriteRefused(format!("no playlist {playlist}")));
        }
        let mut track_no: i64 = tx.query_row(
            "SELECT COALESCE(MAX(TrackNo), 0) FROM djmdSongPlaylist
             WHERE PlaylistID = ?1 AND rb_local_deleted = 0",
            params![playlist],
            |r| r.get(0),
        )?;

        let mut rows = 0;
        let mut usn = 0;
        for (content, (row_id, uuid)) in contents.iter().zip(ids) {
            let already: i64 = tx.query_row(
                "SELECT COUNT(*) FROM djmdSongPlaylist
                 WHERE PlaylistID = ?1 AND ContentID = ?2 AND rb_local_deleted = 0",
                params![playlist, content],
                |r| r.get(0),
            )?;
            if already > 0 {
                continue;
            }
            track_no += 1;
            usn = next_usn(&tx);
            rows += tx.execute(
                "INSERT INTO djmdSongPlaylist
                    (ID, PlaylistID, ContentID, TrackNo, UUID,
                     rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced,
                     usn, rb_local_usn, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, NULL, ?6, ?7, ?7)",
                params![row_id, playlist, content, track_no, uuid, usn, stamp],
            )?;
        }
        if rows > 0 {
            set_counter(&tx, usn)?;
        }
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    /// Removes tracks from a playlist and closes the gaps in `TrackNo`.
    pub fn remove_tracks(&mut self, playlist: &str, contents: &[String]) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut rows = 0;
        let mut usn = 0;
        for content in contents {
            usn = next_usn(&tx);
            rows += tx.execute(
                "UPDATE djmdSongPlaylist SET rb_local_deleted = 1, rb_local_usn = ?1,
                    updated_at = ?2
                 WHERE PlaylistID = ?3 AND ContentID = ?4 AND rb_local_deleted = 0",
                params![usn, stamp, playlist, content],
            )?;
        }
        if rows > 0 {
            usn = renumber(&tx, playlist, &stamp)?;
            set_counter(&tx, usn)?;
        }
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    /// Reorders a playlist to exactly this sequence of tracks.
    ///
    /// Anything in the playlist and not in `order` keeps its place after them,
    /// so a partial order cannot silently drop tracks.
    pub fn reorder(&mut self, playlist: &str, order: &[String]) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut stmt = tx.prepare(
            "SELECT ContentID FROM djmdSongPlaylist
             WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo",
        )?;
        let existing: Vec<String> = stmt
            .query_map(params![playlist], |r| r.get::<_, String>(0))?
            .filter_map(std::result::Result::ok)
            .collect();
        drop(stmt);

        let mut sequence: Vec<String> =
            order.iter().filter(|c| existing.contains(c)).cloned().collect();
        for content in &existing {
            if !sequence.contains(content) {
                sequence.push(content.clone());
            }
        }

        let mut rows = 0;
        let mut usn = 0;
        for (index, content) in sequence.iter().enumerate() {
            usn = next_usn(&tx);
            rows += tx.execute(
                "UPDATE djmdSongPlaylist SET TrackNo = ?1, rb_local_usn = ?2, updated_at = ?3
                 WHERE PlaylistID = ?4 AND ContentID = ?5 AND rb_local_deleted = 0",
                params![i64::try_from(index + 1).unwrap_or(i64::MAX), usn, stamp, playlist, content],
            )?;
        }
        if rows > 0 {
            set_counter(&tx, usn)?;
        }
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    // ---------------------------------------------------------------- import

    /// Adds a file to the library, returning the new track's id.
    ///
    /// The row shape is the one the reference library shows for a track made
    /// on this machine: `rb_data_status` 0 on all 634 of them, `usn` NULL
    /// until the sync assigns one.
    ///
    /// **`Analysed` is left NULL**, which is the column's own default. Every
    /// track in the reference library has been analysed, so it cannot show
    /// what the field holds *before* analysis — and NULL asserts nothing
    /// rather than asserting something unverified. rekordbox sets it when it
    /// analyses the track.
    pub fn import_file(&mut self, path: &Path) -> Result<String> {
        let tags = crate::import::read_tags(path)
            .map_err(|e| DbError::WriteRefused(e.to_string()))?;
        self.prepare()?;

        let id = self.unused_id_below("djmdContent", MAX_CONTENT_ID)?;
        let uuid = self.rng.uuid4();
        let stamp = time::now();
        let folder = path.to_string_lossy().into_owned();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Already in the library: importing again would give one file two
        // rows, and every playlist pointing at the wrong one.
        let existing: i64 = tx.query_row(
            "SELECT COUNT(*) FROM djmdContent WHERE FolderPath = ?1 AND rb_local_deleted = 0",
            params![folder],
            |r| r.get(0),
        )?;
        if existing > 0 {
            return Err(DbError::WriteRefused(format!(
                "{} is already in the library",
                path.display()
            )));
        }

        let artist = intern(&tx, "djmdArtist", "Name", &tags.artist, &mut self.rng, &stamp)?;
        let album = intern(&tx, "djmdAlbum", "Name", &tags.album, &mut self.rng, &stamp)?;
        let genre = intern(&tx, "djmdGenre", "Name", &tags.genre, &mut self.rng, &stamp)?;
        let label = intern(&tx, "djmdLabel", "Name", &tags.label, &mut self.rng, &stamp)?;

        let usn = next_usn(&tx);
        tx.execute(
            "INSERT INTO djmdContent
                (ID, FolderPath, FileNameL, Title, ArtistID, AlbumID, GenreID, LabelID,
                 Length, BitRate, SampleRate, FileSize, ReleaseYear, TrackNo, Commnt,
                 Rating, DJPlayCount, Analysed, UUID,
                 rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced,
                 usn, rb_local_usn, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                     ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     0, 0, NULL, ?16, 0, 0, 0, 0, NULL, ?17, ?18, ?18)",
            params![
                id,
                folder,
                file_name,
                tags.title,
                artist,
                album,
                genre,
                label,
                i64::from(tags.duration_sec),
                i64::from(tags.bitrate),
                i64::from(tags.sample_rate),
                i64::try_from(tags.file_size).unwrap_or(0),
                i64::from(tags.year),
                i64::from(tags.track_no),
                tags.comment,
                uuid,
                usn,
                stamp
            ],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(id)
    }

    /// Finds an unused id below a ceiling, for tables whose ids are smaller.
    fn unused_id_below(&mut self, table: &str, limit: u64) -> Result<String> {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE ID = ?1");
        for _ in 0..ID_ATTEMPTS {
            let candidate = self.rng.numeric_id(limit);
            let taken: i64 =
                self.library.connection().query_row(&sql, params![candidate], |r| r.get(0))?;
            if taken == 0 {
                return Ok(candidate);
            }
        }
        Err(DbError::WriteRefused(format!(
            "could not find an unused id for {table} in {ID_ATTEMPTS} attempts"
        )))
    }

    // ------------------------------------------------------------------ cues

    /// Adds a cue to a track.
    ///
    /// `kind` is rekordbox's own: 0 for a memory cue, 1-3 and 5 for hot cues
    /// A to D, 6-9 for E to H, and 10-17 for I to P. Kind 4 is unused.
    ///
    /// Every column is set to what the reference library shows for a plain,
    /// default-coloured cue — see the module docs. Loops are refused.
    pub fn add_cue(&mut self, content: &str, kind: u8, position_ms: u32) -> Result<String> {
        if kind == 4 || kind > 17 {
            return Err(DbError::WriteRefused(format!(
                "{kind} is not a cue kind rekordbox uses"
            )));
        }
        self.prepare()?;
        let id = self.rng.uuid4();
        let uuid = self.rng.uuid4();
        let stamp = time::now();
        let memory = kind == 0;

        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        // A cue points at a track by id *and* by UUID; both have to match or
        // rekordbox's sync sees a cue with no owner.
        let content_uuid: Option<String> = tx
            .query_row(
                "SELECT UUID FROM djmdContent WHERE ID = ?1 AND rb_local_deleted = 0",
                params![content],
                |r| r.get(0),
            )
            .ok();
        if content_uuid.is_none() {
            return Err(DbError::WriteRefused(format!("no track {content}")));
        }

        let usn = next_usn(&tx);
        tx.execute(
            "INSERT INTO djmdCue
                (ID, ContentID, InMsec, InFrame, InMpegFrame, InMpegAbs,
                 OutMsec, OutFrame, OutMpegFrame, OutMpegAbs,
                 Kind, Color, ColorTableIndex, ActiveLoop, Comment, BeatLoopSize,
                 CueMicrosec, ContentUUID, UUID,
                 rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced,
                 usn, rb_local_usn, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 0, 0, NULL, NULL, NULL, NULL,
                     ?4, ?5, ?6, 0, '', NULL,
                     NULL, ?7, ?8, 0, 0, 0, 0, NULL, ?9, ?10, ?10)",
            params![
                id,
                content,
                i64::from(position_ms),
                i64::from(kind),
                // 255 on a memory cue, -1 on a hot one.
                if memory { 255 } else { -1 },
                // 0 is "no colour"; 21 is the default rekordbox writes when
                // the user has not chosen one.
                if memory { 0 } else { 21 },
                content_uuid,
                uuid,
                usn,
                stamp
            ],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(id)
    }

    /// Adds a loop: a cue with an end as well as a start.
    ///
    /// `beats` is the loop's length in beats, which `BeatLoopSize` carries as
    /// `(beats << 16) | 1`. Passing 0 leaves the length implied by the In and
    /// Out points, which is what most of the library's loops do.
    pub fn add_loop(
        &mut self,
        content: &str,
        kind: u8,
        start_ms: u32,
        end_ms: u32,
        beats: u16,
    ) -> Result<String> {
        if end_ms <= start_ms {
            return Err(DbError::WriteRefused(
                "a loop has to end after it starts".to_owned(),
            ));
        }
        let id = self.add_cue(content, kind, start_ms)?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let usn = next_usn(&tx);
        // The low half is always 1 across every value in the reference
        // library; it reads as the denominator of a beats-per-loop fraction.
        let size = if beats == 0 { 0_i64 } else { (i64::from(beats) << 16) | 1 };
        tx.execute(
            "UPDATE djmdCue SET OutMsec = ?1, BeatLoopSize = ?2, rb_local_usn = ?3,
                updated_at = ?4 WHERE ID = ?5",
            params![i64::from(end_ms), size, usn, stamp, id],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(id)
    }

    /// Moves a cue to a new position.
    pub fn move_cue(&mut self, cue: &str, position_ms: u32) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let usn = next_usn(&tx);
        let rows = tx.execute(
            "UPDATE djmdCue SET InMsec = ?1, rb_local_usn = ?2, updated_at = ?3
             WHERE ID = ?4 AND rb_local_deleted = 0",
            params![i64::from(position_ms), usn, stamp, cue],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    /// Soft-deletes a cue.
    pub fn delete_cue(&mut self, cue: &str) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let usn = next_usn(&tx);
        let rows = tx.execute(
            "UPDATE djmdCue SET rb_local_deleted = 1, rb_local_usn = ?1, updated_at = ?2
             WHERE ID = ?3 AND rb_local_deleted = 0",
            params![usn, stamp, cue],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    // -------------------------------------------------------------- metadata

    /// Sets a track's rating, 0 to 5 stars.
    pub fn set_rating(&mut self, content: &str, stars: u8) -> Result<Changed> {
        if stars > 5 {
            return Err(DbError::WriteRefused(format!("{stars} is not a rating between 0 and 5")));
        }
        // rekordbox stores stars as multiples of 51, so five stars is 255.
        self.touch_content(content, "Rating", &Value::Integer(i64::from(stars) * 51))
    }

    /// Sets a track's comment.
    pub fn set_comment(&mut self, content: &str, comment: &str) -> Result<Changed> {
        self.touch_content(content, "Commnt", &Value::Text(comment.to_owned()))
    }

    /// Sets a track's colour, or clears it with `None`.
    pub fn set_color(&mut self, content: &str, color: Option<&str>) -> Result<Changed> {
        self.touch_content(
            content,
            "ColorID",
            &color.map_or(Value::Null, |c| Value::Text(c.to_owned())),
        )
    }

    /// Points a track at a different file.
    ///
    /// For a track whose audio has moved. Only the location changes — the
    /// analysis, cues and playlist memberships all key off the track's id and
    /// stay where they are.
    pub fn relocate(&mut self, content: &str, path: &Path) -> Result<Changed> {
        if !path.is_file() {
            return Err(DbError::WriteRefused(format!(
                "{} is not a file; a track must point at one",
                path.display()
            )));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let full = path.to_string_lossy().into_owned();
        // Two columns, so two statements under one prepare each; the USN and
        // stamp bookkeeping happens twice and the later one wins, which is
        // what rekordbox's own rows look like after an edit.
        self.touch_content(content, "FolderPath", &Value::Text(full))?;
        self.touch_content(content, "FileNameL", &Value::Text(name))
    }

    /// Soft-deletes a track and every playlist membership pointing at it.
    pub fn delete_track(&mut self, content: &str) -> Result<Changed> {
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut playlists = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT DISTINCT PlaylistID FROM djmdSongPlaylist
                 WHERE ContentID = ?1 AND rb_local_deleted = 0",
            )?;
            playlists.extend(
                stmt.query_map(params![content], |r| r.get::<_, String>(0))?
                    .filter_map(std::result::Result::ok),
            );
        }

        let mut usn = next_usn(&tx);
        let mut rows = tx.execute(
            "UPDATE djmdSongPlaylist SET rb_local_deleted = 1, rb_local_usn = ?1, updated_at = ?2
             WHERE ContentID = ?3 AND rb_local_deleted = 0",
            params![usn, stamp, content],
        )?;
        // Each affected playlist has to close its gaps, or TrackNo stops being
        // contiguous and rekordbox renders the playlist with holes. The USN it
        // hands back is discarded: `next_usn` below takes the maximum across
        // the tables, so it already accounts for whatever renumbering wrote.
        for playlist in &playlists {
            renumber(&tx, playlist, &stamp)?;
        }
        usn = next_usn(&tx);
        rows += tx.execute(
            "UPDATE djmdContent SET rb_local_deleted = 1, rb_local_usn = ?1, updated_at = ?2
             WHERE ID = ?3 AND rb_local_deleted = 0",
            params![usn, stamp, content],
        )?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    // --------------------------------------------------------------- plumbing

    /// One-column update on a playlist, with the bookkeeping attached.
    fn touch_playlist(&mut self, id: &str, column: &str, value: &Value) -> Result<Changed> {
        self.touch("djmdPlaylist", id, column, value)
    }

    /// One-column update on a track, with the bookkeeping attached.
    fn touch_content(&mut self, id: &str, column: &str, value: &Value) -> Result<Changed> {
        self.touch("djmdContent", id, column, value)
    }

    /// Sets one column, bumps the USN, stamps `updated_at`, moves the counter.
    ///
    /// The column name is not user input — it comes from the call sites in this
    /// file — but it is still checked against a list, because a name reaching
    /// this through a future caller would be a SQL injection straight into the
    /// user's library.
    fn touch(&mut self, table: &str, id: &str, column: &str, value: &Value) -> Result<Changed> {
        if !WRITABLE_COLUMNS.contains(&column) {
            return Err(DbError::WriteRefused(format!("{column} is not a writable column")));
        }
        self.prepare()?;
        let stamp = time::now();
        let tx = self.library.connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let usn = next_usn(&tx);
        let sql = format!(
            "UPDATE {table} SET {column} = ?1, rb_local_usn = ?2, updated_at = ?3
             WHERE ID = ?4 AND rb_local_deleted = 0"
        );
        let rows = tx.execute(&sql, params![value, usn, stamp, id])?;
        set_counter(&tx, usn)?;
        tx.commit()?;
        Ok(Changed { rows, usn })
    }

    /// Runs before every transaction.
    ///
    /// rekordbox can be launched between one edit and the next, so the check
    /// made when the session opened is not enough. The backup is taken once,
    /// before the first write of the session.
    fn prepare(&mut self) -> Result<()> {
        // Only the installed library needs this: rekordbox holds that file's
        // WAL, and nothing else's.
        if self.library.location().is_real_install && is_rekordbox_running() {
            return Err(DbError::WriteRefused(
                "rekordbox is running. Quit it before making changes.".to_owned(),
            ));
        }
        if !self.backup_taken {
            self.back_up()?;
            self.backup_taken = true;
        }
        Ok(())
    }

    /// Copies the library aside, keeping the last few.
    ///
    /// `VACUUM INTO` rather than `sqlite3_backup`: `SQLCipher` refuses the backup
    /// API on an encrypted database outright ("backup is not supported with
    /// encrypted databases"). `VACUUM INTO` writes a complete copy encrypted
    /// with the same key, and unlike a file copy it does not miss the WAL.
    fn back_up(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.backup_dir)
            .map_err(|e| DbError::Open(format!("{}: {e}", self.backup_dir.display())))?;
        let name = format!("master-{}.db", time::now().replace([' ', ':', '+', '.'], "-"));
        let target = self.backup_dir.join(name);
        let path = target.to_string_lossy().into_owned();
        // VACUUM INTO refuses to overwrite, so a name collision fails loudly
        // rather than destroying an existing backup.
        self.library.connection().execute("VACUUM INTO ?1", params![path])?;
        prune_backups(&self.backup_dir, BACKUPS_KEPT);
        tracing::info!(path = %target.display(), "backed up the library before writing");
        Ok(())
    }

    /// Finds an id no row in `table` is using.
    fn unused_id(&mut self, table: &str) -> Result<String> {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE ID = ?1");
        for _ in 0..ID_ATTEMPTS {
            let candidate = self.rng.numeric_id(MAX_PLAYLIST_ID);
            let taken: i64 =
                self.library.connection().query_row(&sql, params![candidate], |r| r.get(0))?;
            if taken == 0 {
                return Ok(candidate);
            }
        }
        Err(DbError::WriteRefused(format!(
            "could not find an unused id for {table} in {ID_ATTEMPTS} attempts"
        )))
    }
}

/// Finds a lookup row by name, or makes one, returning its id.
///
/// An empty name is `NO_ID` — the empty string — because rekordbox leaves the
/// reference off rather than pointing at a blank row.
fn intern(
    conn: &Connection,
    table: &str,
    column: &str,
    name: &str,
    rng: &mut Rng,
    stamp: &str,
) -> Result<Option<String>> {
    if name.is_empty() {
        return Ok(None);
    }
    let found: Option<String> = conn
        .query_row(
            &format!("SELECT ID FROM {table} WHERE {column} = ?1 AND rb_local_deleted = 0"),
            params![name],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = found {
        return Ok(Some(id));
    }
    let id = rng.numeric_id(MAX_PLAYLIST_ID);
    let usn = next_usn(conn);
    conn.execute(
        &format!(
            "INSERT INTO {table} (ID, {column}, UUID,
                rb_data_status, rb_local_data_status, rb_local_deleted, rb_local_synced,
                usn, rb_local_usn, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 0, 0, 0, NULL, ?4, ?5, ?5)"
        ),
        params![id, name, rng.uuid4(), usn, stamp],
    )?;
    set_counter(conn, usn)?;
    Ok(Some(id))
}

/// Whether a playlist or folder exists and is not deleted.
fn node_exists(conn: &Connection, id: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM djmdPlaylist WHERE ID = ?1 AND rb_local_deleted = 0",
        params![id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Whether `candidate` sits somewhere under `ancestor`.
fn is_descendant(conn: &Connection, candidate: &str, ancestor: &str) -> bool {
    let mut at = candidate.to_owned();
    // The tree is shallow, but a corrupt parent chain could loop; the bound
    // makes that terminate instead of hanging.
    for _ in 0..256 {
        if at == ROOT {
            return false;
        }
        let parent: Option<String> = conn
            .query_row("SELECT ParentID FROM djmdPlaylist WHERE ID = ?1", params![at], |r| r.get(0))
            .ok();
        match parent {
            Some(p) if p == ancestor => return true,
            Some(p) => at = p,
            None => return false,
        }
    }
    false
}

/// The next local USN.
///
/// `max(the registry counter, the largest USN in use) + 1`. Taking the larger
/// of the two matters: the counter has been observed lagging the table maximum,
/// and reusing a USN makes rekordbox's sync skip the row.
fn next_usn(conn: &Connection) -> i64 {
    let counter: i64 = conn
        .query_row(
            "SELECT COALESCE(int_1, 0) FROM agentRegistry WHERE registry_id = 'localUpdateCount'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut highest = counter;
    for table in USN_TABLES {
        let max: i64 = conn
            .query_row(&format!("SELECT COALESCE(MAX(rb_local_usn), 0) FROM {table}"), [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        highest = highest.max(max);
    }
    highest + 1
}

/// Writes the registry counter. Always last in a transaction, so a crash
/// leaves the counter behind the rows rather than ahead of them — behind is
/// recoverable by taking the maximum, ahead silently skips a row.
fn set_counter(conn: &Connection, usn: i64) -> Result<()> {
    conn.execute(
        "UPDATE agentRegistry SET int_1 = ?1, updated_at = ?2 WHERE registry_id = 'localUpdateCount'",
        params![usn, time::now()],
    )?;
    Ok(())
}

/// Renumbers a playlist's `TrackNo` to 1..N in its current order.
fn renumber(conn: &Connection, playlist: &str, stamp: &str) -> Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT ID FROM djmdSongPlaylist WHERE PlaylistID = ?1 AND rb_local_deleted = 0
         ORDER BY TrackNo",
    )?;
    let ids: Vec<String> = stmt
        .query_map(params![playlist], |r| r.get::<_, String>(0))?
        .filter_map(std::result::Result::ok)
        .collect();
    drop(stmt);

    let mut usn = next_usn(conn);
    for (index, id) in ids.iter().enumerate() {
        let wanted = i64::try_from(index + 1).unwrap_or(i64::MAX);
        // Only touch rows whose number actually moves: an untouched row should
        // not get a new USN and look changed to the sync.
        let current: i64 = conn.query_row(
            "SELECT TrackNo FROM djmdSongPlaylist WHERE ID = ?1",
            params![id],
            |r| r.get(0),
        )?;
        if current == wanted {
            continue;
        }
        usn = next_usn(conn);
        conn.execute(
            "UPDATE djmdSongPlaylist SET TrackNo = ?1, rb_local_usn = ?2, updated_at = ?3
             WHERE ID = ?4",
            params![wanted, usn, stamp, id],
        )?;
    }
    Ok(usn)
}

/// Keeps the newest `keep` backups and removes the rest.
fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("master-"))
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("db"))
        })
        .collect();
    // The name carries the timestamp, so sorting by name sorts by age.
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    for path in backups.into_iter().take(excess) {
        drop(std::fs::remove_file(path));
    }
}
