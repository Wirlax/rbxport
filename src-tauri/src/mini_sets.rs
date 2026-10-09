//! This fork's own: the work behind the `place mini sets` AppleScript
//! command, which the fork's MCP server sends once Ronan has confirmed the
//! blocks in a conversation. Where they go is `rbl_db::mini_sets`.

use std::sync::Arc;

use rbl_db::mini_sets::{Placement, Separator};
use rbl_db::write::{Writer, ROOT};
use rbl_db::DbError;
use tauri::State;

use crate::commands::{edit, Touched};
use crate::error::AppResult;
use crate::scripting::model::ScriptValue;
use crate::state::AppState;

/// Where the blocks go.
pub(crate) enum Target {
    /// A playlist already in the library, by id.
    Playlist(String),
    /// A new playlist at the top of the tree, by name.
    New(String),
    /// The block after a separator in a playlist, by id, given new tracks;
    /// none takes it out.
    Change { playlist: String, separator: Separator },
}

/// Places the blocks as any playlist edit is made — under the edit gate, the
/// index refreshed and the window told — and answers the title of the
/// separator each block went after: for a change, the block's, or nothing
/// when it was taken out.
pub(crate) async fn place<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    target: Target,
    blocks: Vec<Vec<String>>,
) -> AppResult<Vec<String>> {
    let placed = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let out = Arc::clone(&placed);
    edit(app, state, "place_mini_sets", Touched::Playlists, move |writer| {
        let titles = match &target {
            Target::Playlist(id) => titles(&writer.add_mini_sets(id, &blocks)?.1),
            Target::New(name) => titles(&into_new_playlist(writer, name, &blocks)?),
            Target::Change { playlist, separator } => {
                let [tracks] = blocks.as_slice() else {
                    return Err(DbError::WriteRefused("a change is to one block".to_owned()));
                };
                writer.change_mini_set(playlist, *separator, tracks)?;
                if tracks.is_empty() { Vec::new() } else { vec![separator.title()] }
            }
        };
        *out.lock() = titles;
        Ok(())
    })
    .await?;
    let titles = std::mem::take(&mut *placed.lock());
    Ok(titles)
}

fn titles(placements: &[Placement]) -> Vec<String> {
    placements.iter().map(|p| p.separator.title()).collect()
}

/// A new playlist at the top of the tree holding just the blocks.
///
/// Refused when one there already has the name, so a set is never split
/// across two lookalikes; taken back out when the blocks are refused, so a
/// refusal leaves nothing behind.
fn into_new_playlist(writer: &mut Writer, name: &str, blocks: &[Vec<String>]) -> Result<Vec<Placement>, DbError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::WriteRefused("a new playlist needs a name".to_owned()));
    }
    if writer.child_named(ROOT, name)?.is_some() {
        return Err(DbError::WriteRefused(format!("a playlist named {name} is already at the top of the tree")));
    }
    let id = writer.create_playlist(name, ROOT)?;
    match writer.add_mini_sets(&id, blocks) {
        Ok((_, placements)) => Ok(placements),
        Err(refused) => {
            if let Err(error) = writer.delete_playlist(&id) {
                tracing::warn!(%error, playlist = %id, "an empty mini-set playlist could not be taken back out");
            }
            Err(refused)
        }
    }
}

/// The blocks a script names: a list of lists of track ids, as text or as
/// numbers. `None` for anything else, a flat list included.
pub(crate) fn blocks_of(value: &ScriptValue) -> Option<Vec<Vec<String>>> {
    let ScriptValue::List(blocks) = value else { return None };
    blocks
        .iter()
        .map(|block| {
            let ScriptValue::List(tracks) = block else { return None };
            tracks
                .iter()
                .map(|track| match track {
                    ScriptValue::Text(id) => id.trim().parse::<u64>().ok(),
                    ScriptValue::Int(id) => u64::try_from(*id).ok(),
                    _ => None,
                })
                .map(|id| id.map(|id| id.to_string()))
                .collect()
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use rbl_db::fixture::{self, track_id, Shape};

    use super::*;

    fn text(s: &str) -> ScriptValue {
        ScriptValue::Text(s.to_owned())
    }

    #[test]
    fn blocks_are_a_list_of_lists_of_ids() {
        let value = ScriptValue::List(vec![
            ScriptValue::List(vec![text("12"), text(" 34 ")]),
            ScriptValue::List(vec![ScriptValue::Int(56)]),
        ]);
        assert_eq!(blocks_of(&value).unwrap(), [vec!["12".to_owned(), "34".to_owned()], vec!["56".to_owned()]]);
    }

    #[test]
    fn anything_else_is_not_blocks() {
        assert_eq!(blocks_of(&ScriptValue::List(vec![text("12"), text("34")])), None);
        assert_eq!(blocks_of(&ScriptValue::List(vec![ScriptValue::List(vec![text("Boulder")])])), None);
        assert_eq!(blocks_of(&ScriptValue::List(vec![ScriptValue::List(vec![ScriptValue::Int(-1)])])), None);
        assert_eq!(blocks_of(&text("12")), None);
    }

    /// A fixture whose tracks 20 and 21 are `SEPARATORBREMSEN` and `100`.
    fn writer() -> (tempfile::TempDir, Writer) {
        let dir = tempfile::tempdir().unwrap();
        let location = fixture::build(dir.path(), Shape::default()).unwrap();
        let writer = Writer::open(location, dir.path().join("backups")).unwrap();
        for (index, separator) in [(20, Separator::Head), (21, Separator::Numbered(100))] {
            writer
                .library()
                .connection()
                .execute("UPDATE djmdContent SET Title = ?1 WHERE ID = ?2", (separator.title(), track_id(index)))
                .unwrap();
        }
        (dir, writer)
    }

    fn named(writer: &Writer, name: &str) -> Option<String> {
        writer.child_named(ROOT, name).unwrap()
    }

    #[test]
    fn a_new_playlist_holds_just_the_blocks() {
        let (_dir, mut writer) = writer();
        let blocks = vec![vec![track_id(0), track_id(1)], vec![track_id(2)]];
        let placed = into_new_playlist(&mut writer, " Twelve 2 ", &blocks).unwrap();
        assert_eq!(placed.iter().map(|p| p.separator).collect::<Vec<_>>(), [Separator::Head, Separator::Numbered(100)]);
        let id = named(&writer, "Twelve 2").unwrap();
        let entries = rbl_db::mini_sets::entries(writer.library().connection(), &id).unwrap();
        let contents: Vec<String> = entries.into_iter().map(|e| e.content).collect();
        assert_eq!(contents, [track_id(20), track_id(0), track_id(1), track_id(21), track_id(2)]);
    }

    #[test]
    fn a_refused_new_playlist_leaves_nothing_behind() {
        let (_dir, mut writer) = writer();
        let refused = into_new_playlist(&mut writer, "Twelve 2", &[vec!["999999".to_owned()]]);
        assert!(matches!(refused, Err(DbError::WriteRefused(_))));
        assert_eq!(named(&writer, "Twelve 2"), None);
    }

    #[test]
    fn a_name_already_at_the_top_is_refused() {
        let (_dir, mut writer) = writer();
        writer.create_playlist("Twelve 2", ROOT).unwrap();
        let refused = into_new_playlist(&mut writer, "Twelve 2", &[vec![track_id(0)]]);
        match refused {
            Err(DbError::WriteRefused(reason)) => assert!(reason.contains("already at the top"), "{reason}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}
