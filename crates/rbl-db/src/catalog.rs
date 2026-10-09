//! This fork's own: what the fork's MCP server reads of the library — every
//! live track with the fields a set is built from, the playlist tree, and
//! each playlist's tracks in order. Read in three queries per request, so
//! what it answers is never older than the question.

use rusqlite::Connection;

use crate::Result;

/// A live track, with its lookups resolved to names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub remixer: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    /// `djmdKey.ScaleName`: `Ebm`, `F#`…; empty when the track has no key.
    pub key: String,
    /// BPM x100, as stored; 0 when not analysed.
    pub bpm_x100: i64,
    /// Seconds.
    pub length: i64,
    /// 0 to 5 stars.
    pub rating: i64,
    pub comment: String,
    pub year: i64,
    pub play_count: i64,
}

/// Every live track, by id in numeric order.
pub fn tracks(conn: &Connection) -> Result<Vec<CatalogTrack>> {
    let mut stmt = conn.prepare(
        "SELECT c.ID, COALESCE(c.Title, ''), COALESCE(ar.Name, ''), COALESCE(rm.Name, ''),
                COALESCE(al.Name, ''), COALESCE(g.Name, ''), COALESCE(l.Name, ''),
                COALESCE(k.ScaleName, ''), CAST(COALESCE(c.BPM, 0) AS INTEGER),
                CAST(COALESCE(c.Length, 0) AS INTEGER), CAST(COALESCE(c.Rating, 0) AS INTEGER),
                COALESCE(c.Commnt, ''), CAST(COALESCE(c.ReleaseYear, 0) AS INTEGER),
                CAST(COALESCE(c.DJPlayCount, 0) AS INTEGER)
         FROM djmdContent c
         LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID AND ar.rb_local_deleted = 0
         LEFT JOIN djmdArtist rm ON rm.ID = c.RemixerID AND rm.rb_local_deleted = 0
         LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID AND al.rb_local_deleted = 0
         LEFT JOIN djmdGenre g ON g.ID = c.GenreID AND g.rb_local_deleted = 0
         LEFT JOIN djmdLabel l ON l.ID = c.LabelID AND l.rb_local_deleted = 0
         LEFT JOIN djmdKey k ON k.ID = c.KeyID AND k.rb_local_deleted = 0
         WHERE c.rb_local_deleted = 0
         ORDER BY CAST(c.ID AS INTEGER), c.ID",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CatalogTrack {
            id: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            remixer: row.get(3)?,
            album: row.get(4)?,
            genre: row.get(5)?,
            label: row.get(6)?,
            key: row.get(7)?,
            bpm_x100: row.get(8)?,
            length: row.get(9)?,
            rating: row.get(10)?,
            comment: row.get(11)?,
            year: row.get(12)?,
            play_count: row.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A playlist, folder or intelligent playlist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogNode {
    pub id: String,
    pub name: String,
    /// `root` at the top.
    pub parent: String,
    /// [`crate::write::ATTRIBUTE_PLAYLIST`], `_FOLDER` or `_SMART`.
    pub attribute: i64,
    pub seq: i64,
}

/// Every live playlist and folder, siblings in their order.
pub fn nodes(conn: &Connection) -> Result<Vec<CatalogNode>> {
    let mut stmt = conn.prepare(
        "SELECT ID, COALESCE(Name, ''), COALESCE(ParentID, 'root'),
                CAST(COALESCE(Attribute, 0) AS INTEGER), CAST(COALESCE(Seq, 0) AS INTEGER)
         FROM djmdPlaylist WHERE rb_local_deleted = 0
         ORDER BY ParentID, Seq, ID",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CatalogNode { id: row.get(0)?, name: row.get(1)?, parent: row.get(2)?, attribute: row.get(3)?, seq: row.get(4)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Every playlist membership as `(playlist, track)`, each playlist's in order.
pub fn memberships(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT PlaylistID, ContentID FROM djmdSongPlaylist
         WHERE rb_local_deleted = 0
         ORDER BY PlaylistID, TrackNo, ID",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
