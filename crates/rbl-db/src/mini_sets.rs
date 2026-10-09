//! This fork's own: playlists laid out in mini-sets, the way Ronan builds
//! his sets.
//!
//! A mini-set is a block of a few tracks that mix into one another, picked
//! live as a whole. Each block follows a separator: a one-second track titled
//! `SEPARATORBREMSEN` before the first block, then `SEPARATORBREMSEN 100`,
//! `099`… counting down. Every separator is a file of its own because a
//! playlist holds a track once ([`crate::write::Writer::add_tracks`] skips one
//! already there, as rekordbox's "Skip" does). A separator with nothing after
//! it is a slot kept for a block to come.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{params, Connection};

use crate::{DbError, Result};

/// What every separator's title starts with.
pub const SEPARATOR_TITLE: &str = "SEPARATORBREMSEN";

/// A separator, as its title names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Separator {
    /// `SEPARATORBREMSEN`, before a playlist's first block.
    Head,
    /// `SEPARATORBREMSEN 097`.
    Numbered(u16),
}

impl Separator {
    /// The separator a track title names, if it names one.
    #[must_use]
    pub fn parse(title: &str) -> Option<Self> {
        let rest = title.trim().strip_prefix(SEPARATOR_TITLE)?;
        if rest.is_empty() {
            return Some(Self::Head);
        }
        let digits = rest.strip_prefix(' ')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok().map(Self::Numbered)
    }

    /// The title it goes by.
    #[must_use]
    pub fn title(self) -> String {
        match self {
            Self::Head => SEPARATOR_TITLE.to_owned(),
            Self::Numbered(n) => format!("{SEPARATOR_TITLE} {n:03}"),
        }
    }
}

/// One row of a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The membership row, which keeps its id when it moves.
    pub row: String,
    pub content: String,
    pub track_no: i64,
    pub separator: Option<Separator>,
}

/// A playlist's rows in order, its separators told apart by their titles.
pub fn entries(conn: &Connection, playlist: &str) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT sp.ID, sp.ContentID, COALESCE(sp.TrackNo, 0), COALESCE(c.Title, '')
         FROM djmdSongPlaylist sp
         LEFT JOIN djmdContent c ON c.ID = sp.ContentID AND c.rb_local_deleted = 0
         WHERE sp.PlaylistID = ?1 AND sp.rb_local_deleted = 0
         ORDER BY sp.TrackNo, sp.ID",
    )?;
    let rows = stmt.query_map(params![playlist], |row| {
        let title: String = row.get(3)?;
        Ok(Entry { row: row.get(0)?, content: row.get(1)?, track_no: row.get(2)?, separator: Separator::parse(&title) })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Every separator track in the library. Two tracks with one title: the
/// lower id, so the choice is the same every time.
pub fn separators(conn: &Connection) -> Result<BTreeMap<Separator, String>> {
    let mut stmt = conn.prepare(
        "SELECT ID, COALESCE(Title, '') FROM djmdContent
         WHERE rb_local_deleted = 0 AND Title LIKE ?1
         ORDER BY CAST(ID AS INTEGER), ID",
    )?;
    let rows = stmt.query_map(params![format!("{SEPARATOR_TITLE}%")], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut found = BTreeMap::new();
    for row in rows {
        let (id, title) = row?;
        if let Some(separator) = Separator::parse(&title) {
            found.entry(separator).or_insert(id);
        }
    }
    Ok(found)
}

/// Where one new block went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub separator: Separator,
    /// Into a slot the playlist kept for it, rather than on the end after a
    /// separator added for it.
    pub reserved: bool,
}

/// One place in the playlist once the blocks are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// A row already there, by its index in the entries.
    Kept(usize),
    /// A track added, by content id.
    Added(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub order: Vec<Slot>,
    pub placements: Vec<Placement>,
}

/// A separator with nothing after it before the next separator or the end.
fn is_slot(entries: &[Entry], index: usize) -> bool {
    entries.get(index).is_some_and(|e| e.separator.is_some())
        && entries.get(index + 1).is_none_or(|next| next.separator.is_some())
}

/// Where `blocks` go in a playlist holding `entries`: each into the next
/// separator with nothing after it, then on the end after a separator of its
/// own — `SEPARATORBREMSEN` in a playlist with none yet, then the highest
/// number the playlist does not hold. The numbers only make each separator a
/// file of its own (Ronan: they mean nothing to him), so one freed by
/// [`change`] is taken again rather than the count running down to nothing.
///
/// Refused, with nothing to write, for an empty block, a separator or a track
/// already in the playlist inside a block, a track in two blocks, or a
/// library out of separators.
pub fn plan(entries: &[Entry], separators: &BTreeMap<Separator, String>, blocks: &[Vec<String>]) -> Result<Plan> {
    if blocks.is_empty() {
        return Err(DbError::WriteRefused("no blocks to place".to_owned()));
    }
    let present: HashSet<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    let is_separator: HashSet<&str> = separators.values().map(String::as_str).collect();
    let mut seen = HashSet::new();
    for (n, block) in (1..).zip(blocks) {
        if block.is_empty() {
            return Err(DbError::WriteRefused(format!("block {n} is empty")));
        }
        for track in block {
            if is_separator.contains(track.as_str()) {
                return Err(DbError::WriteRefused(format!("block {n}: track {track} is a separator")));
            }
            if present.contains(track.as_str()) {
                return Err(DbError::WriteRefused(format!(
                    "block {n}: track {track} is already in the playlist, which holds a track once"
                )));
            }
            if !seen.insert(track.as_str()) {
                return Err(DbError::WriteRefused(format!("block {n}: track {track} is in an earlier block too")));
            }
        }
    }

    let used: HashSet<Separator> = entries.iter().filter_map(|e| e.separator).collect();
    let mut fresh: Vec<(Separator, &String)> = Vec::new();
    let head = separators.get(&Separator::Head).filter(|_| !used.contains(&Separator::Head));
    if let Some(content) = head.filter(|_| used.is_empty()) {
        fresh.push((Separator::Head, content));
    }
    fresh.extend(
        separators
            .iter()
            .rev()
            .filter(|(separator, _)| matches!(separator, Separator::Numbered(_)) && !used.contains(separator))
            .map(|(separator, content)| (*separator, content)),
    );
    // Last of all, the head separator a playlist that has others lost.
    if let Some(content) = head.filter(|_| !used.is_empty()) {
        fresh.push((Separator::Head, content));
    }
    let slots = (0..entries.len()).filter(|&i| is_slot(entries, i)).count();
    let needed = blocks.len().saturating_sub(slots);
    if fresh.len() < needed {
        return Err(DbError::WriteRefused(format!(
            "no separator left for block {}: add more {SEPARATOR_TITLE} tracks to the library",
            slots + fresh.len() + 1
        )));
    }

    let mut order = Vec::with_capacity(entries.len() + needed + blocks.iter().map(Vec::len).sum::<usize>());
    let mut placements = Vec::with_capacity(blocks.len());
    let mut remaining = blocks.iter();
    for (index, entry) in entries.iter().enumerate() {
        order.push(Slot::Kept(index));
        if let Some(separator) = entry.separator.filter(|_| is_slot(entries, index)) {
            if let Some(block) = remaining.next() {
                order.extend(block.iter().cloned().map(Slot::Added));
                placements.push(Placement { separator, reserved: true });
            }
        }
    }
    for (block, (separator, content)) in remaining.zip(fresh) {
        order.push(Slot::Added(content.clone()));
        order.extend(block.iter().cloned().map(Slot::Added));
        placements.push(Placement { separator, reserved: false });
    }
    Ok(Plan { order, placements })
}

/// One block changed in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub order: Vec<Slot>,
    /// The rows that leave the playlist, by index in the entries.
    pub removed: Vec<usize>,
}

/// The playlist with the block after `separator` holding `tracks` instead,
/// in that order: tracks already in the block keep their rows, the others
/// come and go. No tracks at all takes the block out, separator and all.
///
/// Refused, with nothing to write, for a separator the playlist does not
/// hold, and for a separator, a track given twice, or a track from elsewhere
/// in the playlist among `tracks`.
pub fn change(
    entries: &[Entry],
    separators: &BTreeMap<Separator, String>,
    separator: Separator,
    tracks: &[String],
) -> Result<Change> {
    let at = entries
        .iter()
        .position(|e| e.separator == Some(separator))
        .ok_or_else(|| DbError::WriteRefused(format!("the playlist has no {}", separator.title())))?;
    let end = (at + 1..entries.len()).find(|&i| entries.get(i).is_some_and(|e| e.separator.is_some())).unwrap_or(entries.len());
    let is_separator: HashSet<&str> = separators.values().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut slots = Vec::with_capacity(tracks.len());
    for track in tracks {
        if is_separator.contains(track.as_str()) {
            return Err(DbError::WriteRefused(format!("track {track} is a separator")));
        }
        if !seen.insert(track.as_str()) {
            return Err(DbError::WriteRefused(format!("track {track} is in the block twice")));
        }
        match entries.iter().position(|e| e.content == *track) {
            Some(i) if (at + 1..end).contains(&i) => slots.push(Slot::Kept(i)),
            Some(_) => {
                return Err(DbError::WriteRefused(format!(
                    "track {track} is already elsewhere in the playlist, which holds a track once"
                )))
            }
            None => slots.push(Slot::Added(track.clone())),
        }
    }
    let kept: HashSet<usize> = slots.iter().filter_map(|s| if let Slot::Kept(i) = s { Some(*i) } else { None }).collect();
    let mut removed: Vec<usize> = (at + 1..end).filter(|i| !kept.contains(i)).collect();
    let mut order: Vec<Slot> = (0..at).map(Slot::Kept).collect();
    if tracks.is_empty() {
        removed.insert(0, at);
    } else {
        order.push(Slot::Kept(at));
        order.extend(slots);
    }
    order.extend((end..entries.len()).map(Slot::Kept));
    Ok(Change { order, removed })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn track(content: &str) -> Entry {
        Entry { row: format!("row-{content}"), content: content.to_owned(), track_no: 0, separator: None }
    }

    fn separator(separator: Separator) -> Entry {
        let content = format!("sep-{}", separator.title());
        Entry { row: format!("row-{content}"), content, track_no: 0, separator: Some(separator) }
    }

    /// `SEPARATORBREMSEN` and `001` to `100`, as in Ronan's library.
    fn pool() -> BTreeMap<Separator, String> {
        std::iter::once(Separator::Head)
            .chain((1..=100).map(Separator::Numbered))
            .map(|s| (s, format!("sep-{}", s.title())))
            .collect()
    }

    fn blocks(blocks: &[&[&str]]) -> Vec<Vec<String>> {
        blocks.iter().map(|b| b.iter().map(|&t| t.to_owned()).collect()).collect()
    }

    /// The playlist the plan makes, by content id.
    fn contents(entries: &[Entry], plan: &Plan) -> Vec<String> {
        plan.order
            .iter()
            .map(|slot| match slot {
                Slot::Kept(i) => entries[*i].content.clone(),
                Slot::Added(content) => content.clone(),
            })
            .collect()
    }

    fn refusal<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(DbError::WriteRefused(reason)) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// TWELVE as it was on 2026-10-09: four blocks, then four empty slots.
    fn twelve() -> Vec<Entry> {
        use Separator::{Head, Numbered};
        vec![
            separator(Head), track("2much"), track("isee"), track("bugatti"),
            separator(Numbered(100)), track("booyah"), track("boulder"),
            separator(Numbered(99)), track("slow"), track("gave"),
            separator(Numbered(98)), track("nosurrender"), track("rave"),
            separator(Numbered(97)), separator(Numbered(96)), separator(Numbered(95)), separator(Numbered(94)),
        ]
    }

    #[test]
    fn a_title_names_a_separator_only_in_the_exact_form() {
        assert_eq!(Separator::parse("SEPARATORBREMSEN"), Some(Separator::Head));
        assert_eq!(Separator::parse("SEPARATORBREMSEN 097"), Some(Separator::Numbered(97)));
        assert_eq!(Separator::parse(" SEPARATORBREMSEN 100 "), Some(Separator::Numbered(100)));
        for title in ["SEPARATORBREMSENS", "SEPARATORBREMSEN x", "SEPARATORBREMSEN  97", "SEPARATORBREMSEN -1", "Boulder", ""] {
            assert_eq!(Separator::parse(title), None, "{title:?}");
        }
        assert_eq!(Separator::Numbered(5).title(), "SEPARATORBREMSEN 005");
        assert_eq!(Separator::Head.title(), "SEPARATORBREMSEN");
    }

    #[test]
    fn blocks_fill_the_empty_slots_in_order() {
        let entries = twelve();
        let plan = plan(&entries, &pool(), &blocks(&[&["a", "b"], &["c", "d", "e"]])).unwrap();
        let mut expected: Vec<String> = entries.iter().map(|e| e.content.clone()).collect();
        // After 097 (index 13), then after 096, which has moved two along.
        expected.splice(14..14, ["a".to_owned(), "b".to_owned()]);
        expected.splice(17..17, ["c".to_owned(), "d".to_owned(), "e".to_owned()]);
        assert_eq!(contents(&entries, &plan), expected);
        assert_eq!(plan.placements, vec![
            Placement { separator: Separator::Numbered(97), reserved: true },
            Placement { separator: Separator::Numbered(96), reserved: true },
        ]);
    }

    #[test]
    fn once_the_slots_are_full_blocks_go_on_the_end_counting_down() {
        let entries = twelve();
        let plan = plan(&entries, &pool(), &blocks(&[&["a"], &["b"], &["c"], &["d"], &["e"], &["f"]])).unwrap();
        let made = contents(&entries, &plan);
        assert_eq!(made[made.len() - 4..], ["sep-SEPARATORBREMSEN 093", "e", "sep-SEPARATORBREMSEN 092", "f"]);
        let separators: Vec<(Separator, bool)> = plan.placements.iter().map(|p| (p.separator, p.reserved)).collect();
        assert_eq!(separators, vec![
            (Separator::Numbered(97), true), (Separator::Numbered(96), true),
            (Separator::Numbered(95), true), (Separator::Numbered(94), true),
            (Separator::Numbered(93), false), (Separator::Numbered(92), false),
        ]);
    }

    #[test]
    fn an_empty_playlist_starts_with_the_head_separator_then_100() {
        let plan = plan(&[], &pool(), &blocks(&[&["a", "b"], &["c"]])).unwrap();
        assert_eq!(contents(&[], &plan), ["sep-SEPARATORBREMSEN", "a", "b", "sep-SEPARATORBREMSEN 100", "c"]);
    }

    #[test]
    fn a_playlist_without_separators_keeps_its_tracks_first() {
        let entries = vec![track("x"), track("y")];
        let plan = plan(&entries, &pool(), &blocks(&[&["a"]])).unwrap();
        assert_eq!(contents(&entries, &plan), ["x", "y", "sep-SEPARATORBREMSEN", "a"]);
    }

    #[test]
    fn a_freed_number_is_taken_again_and_a_missing_one_skipped() {
        let entries = vec![separator(Separator::Head), track("x"), separator(Separator::Numbered(99)), track("y")];
        let plan = plan(&entries, &pool(), &blocks(&[&["a"]])).unwrap();
        assert_eq!(plan.placements[0].separator, Separator::Numbered(100));
        let mut pool = pool();
        pool.remove(&Separator::Numbered(100));
        let plan = super::plan(&entries, &pool, &blocks(&[&["a"]])).unwrap();
        assert_eq!(plan.placements[0].separator, Separator::Numbered(98));
    }

    #[test]
    fn a_library_out_of_separators_refuses_the_whole_plan() {
        let pool: BTreeMap<Separator, String> = [Separator::Head, Separator::Numbered(1), Separator::Numbered(2)]
            .into_iter()
            .map(|s| (s, format!("sep-{}", s.title())))
            .collect();
        let entries = vec![separator(Separator::Numbered(2)), track("x")];
        // 001, then the head separator this playlist never had.
        assert_eq!(plan(&entries, &pool, &blocks(&[&["a"], &["b"]])).unwrap().placements.len(), 2);
        let reason = refusal(plan(&entries, &pool, &blocks(&[&["a"], &["b"], &["c"]])));
        assert!(reason.starts_with("no separator left for block 3"), "{reason}");
    }

    #[test]
    fn bad_blocks_are_refused() {
        let entries = twelve();
        assert_eq!(refusal(plan(&entries, &pool(), &[])), "no blocks to place");
        assert_eq!(refusal(plan(&entries, &pool(), &blocks(&[&["a"], &[]]))), "block 2 is empty");
        assert!(refusal(plan(&entries, &pool(), &blocks(&[&["sep-SEPARATORBREMSEN 050"]]))).contains("is a separator"));
        assert!(refusal(plan(&entries, &pool(), &blocks(&[&["boulder"]]))).contains("already in the playlist"));
        assert!(refusal(plan(&entries, &pool(), &blocks(&[&["a"], &["b", "a"]]))).contains("earlier block"));
    }

    /// The playlist a change makes, by content id.
    fn changed(entries: &[Entry], change: &Change) -> Vec<String> {
        change
            .order
            .iter()
            .map(|slot| match slot {
                Slot::Kept(i) => entries[*i].content.clone(),
                Slot::Added(content) => content.clone(),
            })
            .collect()
    }

    fn contents_of(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|e| e.content.clone()).collect()
    }

    #[test]
    fn a_block_changes_in_place_and_the_tracks_it_keeps_keep_their_rows() {
        let entries = twelve();
        let tracks = vec!["boulder".to_owned(), "new".to_owned()];
        let change = change(&entries, &pool(), Separator::Numbered(100), &tracks).unwrap();
        let mut expected = contents_of(&entries);
        expected.splice(5..7, tracks.iter().cloned());
        assert_eq!(changed(&entries, &change), expected);
        assert!(change.order.contains(&Slot::Kept(6)), "Boulder keeps its row");
        assert_eq!(change.removed, [5], "Booyah leaves");

        let swapped = super::change(&entries, &pool(), Separator::Numbered(100), &["boulder".to_owned(), "booyah".to_owned()]).unwrap();
        assert_eq!(swapped.removed, [] as [usize; 0]);
        assert_eq!(swapped.order[5..7], [Slot::Kept(6), Slot::Kept(5)]);
    }

    #[test]
    fn no_tracks_takes_the_block_out_separator_and_all() {
        let entries = twelve();
        let change = change(&entries, &pool(), Separator::Numbered(99), &[]).unwrap();
        assert_eq!(change.removed, [7, 8, 9]);
        let mut expected = contents_of(&entries);
        expected.drain(7..10);
        assert_eq!(changed(&entries, &change), expected);
    }

    #[test]
    fn a_change_is_refused_when_it_would_break_the_playlist() {
        let entries = twelve();
        let one = |t: &str| vec![t.to_owned()];
        assert!(refusal(change(&entries, &pool(), Separator::Numbered(50), &one("a"))).contains("has no SEPARATORBREMSEN 050"));
        assert!(refusal(change(&entries, &pool(), Separator::Numbered(100), &one("2much"))).contains("elsewhere in the playlist"));
        assert!(refusal(change(&entries, &pool(), Separator::Numbered(100), &one("sep-SEPARATORBREMSEN 050"))).contains("is a separator"));
        let twice = vec!["a".to_owned(), "a".to_owned()];
        assert!(refusal(change(&entries, &pool(), Separator::Numbered(100), &twice)).contains("twice"));
    }
}
