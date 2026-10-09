//! This fork's own: the work behind the `place mini sets` AppleScript
//! command, which the fork's MCP server sends once Ronan has confirmed the
//! blocks in a conversation. Where they go is `rbl_db::mini_sets`.

use std::sync::Arc;

use tauri::State;

use crate::commands::{edit, Touched};
use crate::error::AppResult;
use crate::scripting::model::ScriptValue;
use crate::state::AppState;

/// Places the blocks as any playlist edit is made — under the edit gate, the
/// index refreshed and the window told — and answers the title of the
/// separator each block went after.
pub(crate) async fn place<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    playlist: String,
    blocks: Vec<Vec<String>>,
) -> AppResult<Vec<String>> {
    let placed = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let out = Arc::clone(&placed);
    edit(app, state, "place_mini_sets", Touched::Playlists, move |writer| {
        let (_, placements) = writer.add_mini_sets(&playlist, &blocks)?;
        *out.lock() = placements;
        Ok(())
    })
    .await?;
    let titles = placed.lock().iter().map(|p| p.separator.title()).collect();
    Ok(titles)
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
#[allow(clippy::unwrap_used)]
mod tests {
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
}
