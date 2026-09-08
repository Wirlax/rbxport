//! What `djmdCue` holds. READ-ONLY.
//!
//! `cargo run -p rbl-db --example cues`
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

fn main() {
    let db = match rbl_db::Library::open_installed_read_only() {
        Ok(db) => db,
        Err(e) => {
            println!("cannot open library: {e}");
            return;
        }
    };
    let conn = db.connection();

    for (label, sql) in [
        ("cues by Kind (0 = memory, hot cues above)",
         "SELECT Kind, COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0 GROUP BY Kind ORDER BY Kind"),
        ("how many carry a Color, and which",
         "SELECT Color, COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0 GROUP BY Color ORDER BY COUNT(*) DESC LIMIT 8"),
        ("ColorTableIndex values",
         "SELECT ColorTableIndex, COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0 GROUP BY ColorTableIndex ORDER BY COUNT(*) DESC LIMIT 8"),
        ("loops: how many have an OutMsec",
         "SELECT (OutMsec IS NOT NULL AND OutMsec > 0) AS is_loop, COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0 GROUP BY 1"),
        ("tracks with any cue",
         "SELECT COUNT(DISTINCT ContentID) FROM djmdCue WHERE rb_local_deleted = 0"),
        ("most cues on one track",
         "SELECT COUNT(*) FROM djmdCue WHERE rb_local_deleted = 0 GROUP BY ContentID ORDER BY COUNT(*) DESC LIMIT 1"),
        ("a sample track's cues",
         "SELECT Kind, InMsec, OutMsec, Color, ColorTableIndex, Comment FROM djmdCue \
          WHERE rb_local_deleted = 0 AND ContentID = (
            SELECT ContentID FROM djmdCue WHERE rb_local_deleted = 0
            GROUP BY ContentID ORDER BY COUNT(*) DESC LIMIT 1) ORDER BY Kind, InMsec"),
    ] {
        println!("== {label} ==");
        let Ok(mut stmt) = conn.prepare(sql) else {
            println!("  (query failed)\n");
            continue;
        };
        let cols = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Ok(Some(row)) = rows.next() {
            let cells: Vec<String> = (0..cols)
                .map(|i| match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => "NULL".to_owned(),
                    Ok(rusqlite::types::ValueRef::Integer(v)) => v.to_string(),
                    Ok(rusqlite::types::ValueRef::Text(v)) => {
                        String::from_utf8_lossy(v).into_owned()
                    }
                    other => format!("{other:?}"),
                })
                .collect();
            println!("  {}", cells.join(" | "));
        }
        println!();
    }
}
