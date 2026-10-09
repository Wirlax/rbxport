//! Each live track's file, with the artist and album it would be filed
//! under: what Organize Library reads before it moves anything.
//!
//! `FolderPath` is read as stored, not resolved: a cloud-shared or streamed
//! track is told apart by `ContentLink` and `ServiceID`, and its file is
//! never the library's to move (see [`crate::track_path`]).

use rusqlite::{params, Connection, OptionalExtension};

use crate::Result;

/// One live track's file and the names it is filed by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackFile {
    pub id: String,
    /// `FolderPath` as stored: the file's full path.
    pub folder_path: String,
    pub artist: String,
    pub album: String,
    pub content_link: i64,
    pub service_id: i64,
}

/// Every live track, by id in numeric order, so a plan made from it is the
/// same from one run to the next.
pub fn track_files(conn: &Connection) -> Result<Vec<TrackFile>> {
    let mut stmt = conn.prepare(
        "SELECT c.ID, COALESCE(c.FolderPath, ''), COALESCE(a.Name, ''), COALESCE(al.Name, ''),
                CAST(COALESCE(c.ContentLink, 0) AS INTEGER), CAST(COALESCE(c.ServiceID, 0) AS INTEGER)
         FROM djmdContent c
         LEFT JOIN djmdArtist a ON a.ID = c.ArtistID AND a.rb_local_deleted = 0
         LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID AND al.rb_local_deleted = 0
         WHERE c.rb_local_deleted = 0
         ORDER BY CAST(c.ID AS INTEGER), c.ID",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TrackFile {
            id: row.get(0)?,
            folder_path: row.get(1)?,
            artist: row.get(2)?,
            album: row.get(3)?,
            content_link: row.get(4)?,
            service_id: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A live track's `FolderPath`, or `None` when there is no such track.
pub fn folder_path(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT COALESCE(FolderPath, '') FROM djmdContent WHERE ID = ?1 AND rb_local_deleted = 0",
            params![id],
            |row| row.get(0),
        )
        .optional()?)
}
