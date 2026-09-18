//! What an export needs from the library beyond the index's columns.
//!
//! Read straight from the database for the tracks being exported rather than
//! kept in memory for every track: My Tag memberships and the alternative
//! paths of a cloud-synced file are looked at once per export, and the index
//! would carry them for 38,681 rows to answer for 61.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::Result;

/// One row of `djmdMyTag`: a category (`attribute` 1, parent `root`) or a
/// tag under one (`attribute` 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyTagRow {
    pub id: String,
    pub seq: i64,
    pub name: String,
    pub attribute: i64,
    /// The category's id, or `root` for a category.
    pub parent: String,
}

/// Whether a table exists, so a library without My Tags still exports.
fn has_table(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .is_ok_and(|n| n > 0)
}

/// Every live My Tag, categories first in their order, then the tags in
/// theirs.
pub fn my_tags(conn: &Connection) -> Result<Vec<MyTagRow>> {
    if !has_table(conn, "djmdMyTag") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT ID, Seq, Name, Attribute, ParentID FROM djmdMyTag
         WHERE rb_local_deleted = 0 ORDER BY Attribute DESC, Seq, ID",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(MyTagRow {
            id: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            seq: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
            name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            attribute: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            parent: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.filter_map(std::result::Result::ok).filter(|t| !t.id.is_empty()).collect())
}

/// What one exported track needs that the index does not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackExtras {
    /// Other places the file may be, after `FolderPath`: the local copy of a
    /// cloud-synced track (`rb_LocalFolderPath`) and where it was imported
    /// from (`OrgFolderPath`). Empty entries are left out.
    pub alternate_paths: Vec<String>,
    /// The ids of the My Tags on the track.
    pub my_tags: Vec<String>,
}

/// The extras for a set of tracks, keyed by `djmdContent.ID`.
///
/// One query per thousand ids, so a whole-library export stays a handful
/// of statements rather than one per track.
pub fn track_extras(conn: &Connection, ids: &[String]) -> Result<HashMap<String, TrackExtras>> {
    let mut out: HashMap<String, TrackExtras> = HashMap::with_capacity(ids.len());
    let tagged = has_table(conn, "djmdSongMyTag");
    for chunk in ids.chunks(900) {
        let marks = vec!["?"; chunk.len()].join(",");
        let params = rusqlite::params_from_iter(chunk.iter());
        let mut stmt = conn.prepare(&format!(
            "SELECT ID, rb_LocalFolderPath, OrgFolderPath FROM djmdContent WHERE ID IN ({marks})"
        ))?;
        let rows = stmt.query_map(params, |r| {
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })?;
        for (id, local, org) in rows.filter_map(std::result::Result::ok) {
            let extras = out.entry(id).or_default();
            for path in [local, org] {
                if !path.is_empty() && !extras.alternate_paths.contains(&path) {
                    extras.alternate_paths.push(path);
                }
            }
        }
        if tagged {
            let params = rusqlite::params_from_iter(chunk.iter());
            let mut stmt = conn.prepare(&format!(
                "SELECT ContentID, MyTagID FROM djmdSongMyTag
                 WHERE rb_local_deleted = 0 AND ContentID IN ({marks}) ORDER BY ContentID, MyTagID"
            ))?;
            let rows = stmt.query_map(params, |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            })?;
            for (content, tag) in rows.filter_map(std::result::Result::ok) {
                if !tag.is_empty() {
                    out.entry(content).or_default().my_tags.push(tag);
                }
            }
        }
    }
    Ok(out)
}
