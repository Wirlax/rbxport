//! The library as the tools see it: a snapshot read straight from the
//! database for each request, never through the app, so reading works with
//! rbxport closed and rekordbox open.

use std::collections::{BTreeMap, HashMap, HashSet};

use rbl_core::musickey::{self, Key, Mode};
use rbl_db::catalog::{self, CatalogNode, CatalogTrack};
use rbl_db::mini_sets::{self, Placement, Separator};
use rbl_db::write::{ATTRIBUTE_FOLDER, ATTRIBUTE_SMART, ROOT};
use rbl_db::{DbError, Library, LibraryLocation, OpenMode};
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// A block's tempo may drift this far between two tracks before the preview
/// points it out.
const BPM_JUMP: f64 = 3.0;

pub struct Snapshot {
    db: Library,
    tracks: Vec<CatalogTrack>,
    by_id: HashMap<String, usize>,
    nodes: Vec<CatalogNode>,
    /// Each playlist's tracks, in order.
    members: HashMap<String, Vec<String>>,
    /// Each track's playlists, by index into `nodes`, in tree order.
    homes: HashMap<String, Vec<usize>>,
    separators: BTreeMap<Separator, String>,
    separator_of: HashMap<String, Separator>,
}

impl Snapshot {
    /// The library rbxport would open, or `at` for a test.
    pub fn load(at: Option<&LibraryLocation>) -> Result<Self, String> {
        let location = match at {
            Some(location) => location.clone(),
            None => rbl_db::detect().map_err(|e| e.to_string())?,
        };
        let db = Library::open(location, OpenMode::ReadOnly).map_err(|e| e.to_string())?;
        Self::read(db).map_err(|e| e.to_string())
    }

    fn read(db: Library) -> Result<Self, DbError> {
        let conn = db.connection();
        let tracks = catalog::tracks(conn)?;
        let nodes = tree_order(&catalog::nodes(conn)?);
        let separators = mini_sets::separators(conn)?;
        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (playlist, content) in catalog::memberships(conn)? {
            members.entry(playlist).or_default().push(content);
        }
        let mut homes: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, node) in nodes.iter().enumerate() {
            for content in members.get(&node.id).into_iter().flatten() {
                let at = homes.entry(content.clone()).or_default();
                if at.last() != Some(&index) {
                    at.push(index);
                }
            }
        }
        let by_id = tracks.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        let separator_of = separators.iter().map(|(s, id)| (id.clone(), *s)).collect();
        Ok(Self { db, tracks, by_id, nodes, members, homes, separators, separator_of })
    }

    fn track(&self, id: &str) -> Option<&CatalogTrack> {
        self.by_id.get(id).and_then(|&i| self.tracks.get(i))
    }

    fn is_separator(&self, track: &CatalogTrack) -> bool {
        self.separator_of.contains_key(&track.id) || Separator::parse(&track.title).is_some()
    }

    /// A track as a tool shows it.
    pub fn view(&self, track: &CatalogTrack) -> TrackView {
        let key = musickey::parse(&track.key);
        TrackView {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            remixer: track.remixer.clone(),
            bpm: bpm(track),
            key: track.key.clone(),
            camelot: key.map(camelot_code),
            length: format!("{}:{:02}", track.length / 60, track.length % 60),
            rating: track.rating,
            genre: track.genre.clone(),
            comment: track.comment.clone(),
            playlists: self
                .homes
                .get(&track.id)
                .into_iter()
                .flatten()
                .filter_map(|&i| self.nodes.get(i))
                .map(|n| n.name.clone())
                .collect(),
        }
    }

    fn view_of(&self, id: &str) -> Option<TrackView> {
        self.track(id).map(|t| self.view(t))
    }

    // ------------------------------------------------------------ playlists

    /// A playlist or folder by id, or by its name, ignoring case and accents.
    pub fn playlist(&self, wanted: &str) -> Result<&CatalogNode, String> {
        let wanted = wanted.trim();
        if let Some(node) = self.nodes.iter().find(|n| n.id == wanted) {
            return Ok(node);
        }
        let folded = fold(wanted);
        let named: Vec<&CatalogNode> = self.nodes.iter().filter(|n| fold(&n.name) == folded).collect();
        match named.as_slice() {
            [one] => Ok(one),
            [] => Err(format!("No playlist is named {wanted:?}. list_playlists shows them all.")),
            many => Err(format!(
                "Several playlists are named {wanted:?}; say which by id: {}",
                many.iter().map(|n| format!("{} ({})", n.id, self.path(n))).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    /// The id of the playlist at the top of the tree named `name`.
    pub fn top_named(&self, name: &str) -> Option<String> {
        let folded = fold(name);
        self.nodes.iter().find(|n| n.parent == ROOT && fold(&n.name) == folded).map(|n| n.id.clone())
    }

    fn path(&self, node: &CatalogNode) -> String {
        let mut parts = vec![node.name.clone()];
        let mut parent = node.parent.as_str();
        // A cycle cannot happen in a library rbxport opened, but a bound
        // costs nothing.
        for _ in 0..64 {
            let Some(up) = self.nodes.iter().find(|n| n.id == parent) else { break };
            parts.push(up.name.clone());
            parent = up.parent.as_str();
        }
        parts.reverse();
        parts.join(" / ")
    }

    pub fn playlists(&self) -> Vec<PlaylistView> {
        self.nodes
            .iter()
            .map(|node| {
                let tracks = self.members.get(&node.id).map_or(0, Vec::len);
                let layout = self.layout_of(&node.id);
                PlaylistView {
                    id: node.id.clone(),
                    name: node.name.clone(),
                    path: self.path(node),
                    kind: match node.attribute {
                        ATTRIBUTE_FOLDER => "folder",
                        ATTRIBUTE_SMART => "intelligent (its tracks are a rule, not listed here)",
                        _ => "playlist",
                    },
                    tracks,
                    mini_sets: (!layout.blocks.is_empty()).then(|| MiniSetCount {
                        blocks: layout.blocks.iter().filter(|b| !b.tracks.is_empty()).count(),
                        empty_slots: layout.blocks.iter().filter(|b| b.tracks.is_empty()).count(),
                    }),
                }
            })
            .collect()
    }

    fn layout_of(&self, playlist: &str) -> Layout {
        let mut layout = Layout::default();
        for content in self.members.get(playlist).into_iter().flatten() {
            match (self.separator_of.get(content), layout.blocks.last_mut()) {
                (Some(&separator), _) => layout.blocks.push(Block { separator, tracks: Vec::new() }),
                (None, Some(block)) => block.tracks.push(content.clone()),
                (None, None) => layout.loose.push(content.clone()),
            }
        }
        layout
    }

    /// A playlist cut at its separators, each block with its transitions.
    pub fn mini_sets(&self, wanted: &str) -> Result<MiniSetsView, String> {
        let node = self.playlist(wanted)?;
        if node.attribute == ATTRIBUTE_FOLDER {
            return Err(format!("{} is a folder.", node.name));
        }
        let layout = self.layout_of(&node.id);
        Ok(MiniSetsView {
            playlist: node.name.clone(),
            id: node.id.clone(),
            loose: layout.loose.iter().filter_map(|id| self.view_of(id)).collect(),
            blocks: layout
                .blocks
                .iter()
                .filter(|b| !b.tracks.is_empty())
                .map(|b| BlockView {
                    separator: b.separator.title(),
                    tracks: b.tracks.iter().filter_map(|id| self.view_of(id)).collect(),
                    transitions: self.transitions(&b.tracks),
                })
                .collect(),
            empty_slots: layout.blocks.iter().filter(|b| b.tracks.is_empty()).map(|b| b.separator.title()).collect(),
        })
    }

    // --------------------------------------------------------------- search

    /// Tracks matching every filter given, the best text matches first.
    pub fn search(&self, query: &Search) -> Result<Vec<TrackView>, String> {
        let only: Option<HashSet<&str>> = match &query.playlist {
            Some(wanted) => {
                let node = self.playlist(wanted)?;
                Some(self.members.get(&node.id).into_iter().flatten().map(String::as_str).collect())
            }
            None => None,
        };
        let key = match query.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            Some(text) => Some(musickey::parse(text).ok_or_else(|| format!("{text:?} is not a key: use Ebm, F# or 2A."))?),
            None => None,
        };
        let words: Vec<String> = query.text.as_deref().map(fold).unwrap_or_default().split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect();
        let mut found: Vec<(u32, &CatalogTrack)> = self
            .tracks
            .iter()
            .filter(|t| !self.is_separator(t))
            .filter(|t| only.as_ref().is_none_or(|ids| ids.contains(t.id.as_str())))
            .filter(|t| in_range(bpm(t), query.bpm_min, query.bpm_max))
            .filter(|t| key.is_none_or(|k| musickey::parse(&t.key) == Some(k)))
            .filter_map(|t| score(t, &words).map(|s| (s, t)))
            .collect();
        found.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| fold(&x.title).cmp(&fold(&y.title))));
        Ok(found.into_iter().take(query.limit).map(|(_, t)| self.view(t)).collect())
    }

    /// The tracks that mix with `id`: a Camelot-compatible key and a tempo
    /// within `tolerance`, the closest first.
    pub fn compatible(&self, id: &str, playlist: Option<&str>, tolerance: f64, limit: usize) -> Result<Compatible, String> {
        let track = self.track(id).ok_or_else(|| format!("No track has id {id}."))?;
        let key = musickey::parse(&track.key).ok_or_else(|| format!("{} has no key to match.", track.title))?;
        let tempo = bpm(track).ok_or_else(|| format!("{} has no BPM to match.", track.title))?;
        let only: Option<HashSet<&str>> = match playlist {
            Some(wanted) => {
                let node = self.playlist(wanted)?;
                Some(self.members.get(&node.id).into_iter().flatten().map(String::as_str).collect())
            }
            None => None,
        };
        let mut matches: Vec<(Relation, f64, &CatalogTrack)> = self
            .tracks
            .iter()
            .filter(|t| t.id != track.id && !self.is_separator(t))
            .filter(|t| only.as_ref().is_none_or(|ids| ids.contains(t.id.as_str())))
            .filter_map(|t| {
                let other = musickey::parse(&t.key)?;
                let diff = bpm(t)? - tempo;
                let relation = relation(key, other);
                (relation != Relation::Clash && diff.abs() <= tolerance).then_some((relation, diff, t))
            })
            .collect();
        matches.sort_by(|(ra, da, _), (rb, db, _)| ra.cmp(rb).then_with(|| da.abs().total_cmp(&db.abs())));
        Ok(Compatible {
            track: self.view(track),
            matches: matches
                .into_iter()
                .take(limit)
                .map(|(relation, diff, t)| CompatibleView { relation, bpm_change: round2(diff), track: self.view(t) })
                .collect(),
        })
    }

    // -------------------------------------------------------------- preview

    /// Where `blocks` would go, without writing: the same plan the app will
    /// carry out, so a refusal here is the refusal the write would meet.
    pub fn preview(&self, target: &Target, blocks: &[Vec<String>]) -> Result<Preview, String> {
        for id in blocks.iter().flatten() {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || self.track(id).is_none() {
                return Err(format!("No track has id {id:?}. Find tracks with search_tracks."));
            }
        }
        let (name, entries) = match target {
            Target::Playlist(wanted) => {
                let node = self.playlist(wanted)?;
                match node.attribute {
                    ATTRIBUTE_FOLDER => return Err(format!("{} is a folder, not a playlist.", node.name)),
                    ATTRIBUTE_SMART => return Err(format!("{} is an intelligent playlist; its tracks are a rule.", node.name)),
                    _ => {}
                }
                let entries = mini_sets::entries(self.db.connection(), &node.id).map_err(|e| e.to_string())?;
                (node.name.clone(), entries)
            }
            Target::New(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Err("A new playlist needs a name.".to_owned());
                }
                if self.nodes.iter().any(|n| n.parent == ROOT && fold(&n.name) == fold(name)) {
                    return Err(format!("A playlist named {name:?} is already at the top of the tree: add to it, or choose another name."));
                }
                (name.to_owned(), Vec::new())
            }
        };
        let plan = mini_sets::plan(&entries, &self.separators, blocks).map_err(|e| match e {
            DbError::WriteRefused(reason) => reason,
            other => other.to_string(),
        })?;
        Ok(Preview {
            playlist: name,
            new_playlist: matches!(target, Target::New(_)),
            blocks: plan
                .placements
                .iter()
                .zip(blocks)
                .map(|(placement, block)| self.planned(*placement, block))
                .collect(),
        })
    }

    fn planned(&self, placement: Placement, block: &[String]) -> PlannedBlock {
        let transitions = self.transitions(block);
        let warnings = transitions
            .iter()
            .filter_map(|t| {
                let mut said = Vec::new();
                if t.key == Some(Relation::Clash) {
                    said.push(format!("key clash {} → {}", t.from_key, t.to_key));
                }
                if t.bpm_change.is_some_and(|d| d.abs() > BPM_JUMP) {
                    said.push(format!("tempo jump of {:+} BPM", t.bpm_change.unwrap_or(0.0)));
                }
                (!said.is_empty()).then(|| format!("{} → {}: {}", t.from, t.to, said.join(", ")))
            })
            .collect();
        PlannedBlock {
            after: placement.separator.title(),
            into_reserved_slot: placement.reserved,
            tracks: block.iter().filter_map(|id| self.view_of(id)).collect(),
            transitions,
            warnings,
        }
    }

    fn transitions(&self, block: &[String]) -> Vec<Transition> {
        block
            .windows(2)
            .filter_map(|pair| {
                let (from, to) = (self.track(pair.first()?)?, self.track(pair.get(1)?)?);
                let (a, b) = (musickey::parse(&from.key), musickey::parse(&to.key));
                Some(Transition {
                    from: from.title.clone(),
                    to: to.title.clone(),
                    from_key: a.map_or_else(|| "?".to_owned(), camelot_code),
                    to_key: b.map_or_else(|| "?".to_owned(), camelot_code),
                    key: a.zip(b).map(|(a, b)| relation(a, b)),
                    bpm_change: bpm(from).zip(bpm(to)).map(|(a, b)| round2(b - a)),
                })
            })
            .collect()
    }
}

/// Folders before their contents, siblings in order: the tree as the app
/// draws it.
fn tree_order(nodes: &[CatalogNode]) -> Vec<CatalogNode> {
    fn walk(parent: &str, nodes: &[CatalogNode], out: &mut Vec<CatalogNode>, depth: usize) {
        if depth > 64 {
            return;
        }
        let mut children: Vec<&CatalogNode> = nodes.iter().filter(|n| n.parent == parent).collect();
        children.sort_by_key(|n| n.seq);
        for child in children {
            out.push(child.clone());
            walk(&child.id, nodes, out, depth + 1);
        }
    }
    let mut out = Vec::with_capacity(nodes.len());
    walk(ROOT, nodes, &mut out, 0);
    out
}

// ------------------------------------------------------------------ keys

/// The Camelot wheel's minor keys, 1A to 12A, as pitch classes: the same
/// table `rbl_core::musickey` reads codes with, kept here to write them.
const CAMELOT_MINOR: [u8; 12] = [8, 3, 10, 5, 0, 7, 2, 9, 4, 11, 6, 1];

/// A key on the wheel: its number, 1 to 12, and whether it is minor (`A`).
fn camelot(key: Key) -> (u8, bool) {
    let minor = key.mode == Mode::Minor;
    // A major key shares its number with the minor three semitones below.
    let minor_pitch = if minor { key.pitch % 12 } else { (key.pitch + 9) % 12 };
    let number = CAMELOT_MINOR.iter().position(|&p| p == minor_pitch).map_or(0, |i| i + 1);
    (u8::try_from(number).unwrap_or(0), minor)
}

pub fn camelot_code(key: Key) -> String {
    let (number, minor) = camelot(key);
    format!("{number}{}", if minor { 'A' } else { 'B' })
}

/// How two keys mix, from smoothest to not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Same,
    /// Same number, the other letter: the relative major or minor.
    Relative,
    /// One step round the wheel, same letter.
    Adjacent,
    Clash,
}

pub fn relation(a: Key, b: Key) -> Relation {
    let ((na, ma), (nb, mb)) = (camelot(a), camelot(b));
    let steps = (i16::from(na) - i16::from(nb)).rem_euclid(12);
    match (steps, ma == mb) {
        (0, true) => Relation::Same,
        (0, false) => Relation::Relative,
        (1 | 11, true) => Relation::Adjacent,
        _ => Relation::Clash,
    }
}

// ---------------------------------------------------------------- search

/// Lower case, accents and apostrophes gone, everything else not a letter
/// or digit a single space: `Booyah Jale’s Edit` is `booyah jales edit`.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.nfd() {
        match c {
            '\u{300}'..='\u{36f}' | '\'' | '’' | '‘' | '`' => {}
            'æ' | 'Æ' => out.push_str("ae"),
            'œ' | 'Œ' => out.push_str("oe"),
            'ß' => out.push_str("ss"),
            'ø' | 'Ø' => out.push('o'),
            c if c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How well a track matches every word, or `None` when one is missing: a
/// word starting a title word counts most, then the title, then the
/// artist, then anything else.
fn score(track: &CatalogTrack, words: &[String]) -> Option<u32> {
    if words.is_empty() {
        return Some(0);
    }
    let title = fold(&track.title);
    let artist = fold(&track.artist);
    let rest = fold(&[track.remixer.as_str(), &track.album, &track.label, &track.comment].join(" "));
    words.iter().try_fold(0, |total, word| {
        let points = if title.split(' ').any(|w| w.starts_with(word.as_str())) {
            3
        } else if title.contains(word.as_str()) || artist.contains(word.as_str()) {
            2
        } else if rest.contains(word.as_str()) {
            1
        } else {
            return None;
        };
        Some(total + points)
    })
}

fn bpm(track: &CatalogTrack) -> Option<f64> {
    (track.bpm_x100 > 0).then(|| round2(f64::from(i32::try_from(track.bpm_x100).unwrap_or(0)) / 100.0))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn in_range(bpm: Option<f64>, min: Option<f64>, max: Option<f64>) -> bool {
    if min.is_none() && max.is_none() {
        return true;
    }
    bpm.is_some_and(|b| min.is_none_or(|m| b >= m) && max.is_none_or(|m| b <= m))
}

// ----------------------------------------------------------------- shapes

pub struct Search {
    pub text: Option<String>,
    pub playlist: Option<String>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub key: Option<String>,
    pub limit: usize,
}

pub enum Target {
    /// A playlist already in the library, by id or name.
    Playlist(String),
    /// A new playlist at the top of the tree.
    New(String),
}

#[derive(Default)]
struct Layout {
    loose: Vec<String>,
    blocks: Vec<Block>,
}

struct Block {
    separator: Separator,
    tracks: Vec<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if takes a reference")]
fn is_zero(n: &i64) -> bool {
    *n == 0
}

#[derive(Debug, Serialize)]
pub struct TrackView {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remixer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camelot: Option<String>,
    pub length: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub rating: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub genre: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub playlists: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PlaylistView {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub tracks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mini_sets: Option<MiniSetCount>,
}

#[derive(Debug, Serialize)]
pub struct MiniSetCount {
    pub blocks: usize,
    pub empty_slots: usize,
}

#[derive(Debug, Serialize)]
pub struct MiniSetsView {
    pub playlist: String,
    pub id: String,
    /// Tracks before the first separator.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<TrackView>,
    pub blocks: Vec<BlockView>,
    /// Separators with nothing after them: where the next blocks go.
    pub empty_slots: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BlockView {
    pub separator: String,
    pub tracks: Vec<TrackView>,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Serialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub from_key: String,
    pub to_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<Relation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm_change: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Compatible {
    pub track: TrackView,
    pub matches: Vec<CompatibleView>,
}

#[derive(Debug, Serialize)]
pub struct CompatibleView {
    pub relation: Relation,
    pub bpm_change: f64,
    pub track: TrackView,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub playlist: String,
    pub new_playlist: bool,
    pub blocks: Vec<PlannedBlock>,
}

#[derive(Debug, Serialize)]
pub struct PlannedBlock {
    /// The separator the block goes after.
    pub after: String,
    /// It fills a slot the playlist kept, rather than going on the end.
    pub into_reserved_slot: bool,
    pub tracks: Vec<TrackView>,
    pub transitions: Vec<Transition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use rbl_db::fixture::{self, playlist_id, track_id, Shape};
    use rbl_db::write::Writer;

    use super::*;

    #[test]
    fn camelot_codes_read_back_as_the_same_key() {
        for number in 1..=12 {
            for letter in ['A', 'B'] {
                let code = format!("{number}{letter}");
                let key = musickey::parse(&code).unwrap();
                assert_eq!(camelot_code(key), code);
            }
        }
        assert_eq!(camelot_code(musickey::parse("Ebm").unwrap()), "2A");
        assert_eq!(camelot_code(musickey::parse("Eb").unwrap()), "5B");
    }

    #[test]
    fn relations_follow_the_wheel() {
        let key = |s: &str| musickey::parse(s).unwrap();
        assert_eq!(relation(key("Ebm"), key("D#m")), Relation::Same);
        assert_eq!(relation(key("Am"), key("C")), Relation::Relative);
        assert_eq!(relation(key("Gm"), key("Dm")), Relation::Adjacent);
        assert_eq!(relation(key("1A"), key("12A")), Relation::Adjacent, "the wheel wraps");
        assert_eq!(relation(key("Ebm"), key("Eb")), Relation::Clash, "TWELVE's Slow It Down → Gave Birth");
    }

    #[test]
    fn folding_ignores_case_accents_and_apostrophes() {
        assert_eq!(fold("Booyah Jale’s Edit"), "booyah jales edit");
        assert_eq!(fold("ÆON:MODE"), "aeon mode");
        assert_eq!(fold("  Beyoncé  — Déjà Vu "), "beyonce deja vu");
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        location: LibraryLocation,
    }

    /// The default fixture with keys, an artist, and tracks 30 to 33 as the
    /// separators `SEPARATORBREMSEN`, `100`, `099` and `098`; playlist 1
    /// holds a mini-set and two empty slots.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let location = fixture::build(dir.path(), Shape::default()).unwrap();
        let mut writer = Writer::open(location.clone(), dir.path().join("backups")).unwrap();
        let conn = writer.library().connection();
        for (index, title) in [(30, "SEPARATORBREMSEN"), (31, "SEPARATORBREMSEN 100"), (32, "SEPARATORBREMSEN 099"), (33, "SEPARATORBREMSEN 098")] {
            conn.execute("UPDATE djmdContent SET Title = ?1 WHERE ID = ?2", (title, track_id(index))).unwrap();
        }
        conn.execute("INSERT INTO djmdKey (ID, ScaleName, Seq, created_at, updated_at)
                 VALUES ('k1', 'Ebm', 1, '2026-10-09', '2026-10-09'), ('k2', 'Eb', 2, '2026-10-09', '2026-10-09')", ()).unwrap();
        for (index, key, bpm, title) in [(10, "k1", 17_400, "Boulder"), (11, "k1", 17_500, "Booyah Jale’s Edit"), (12, "k2", 17_400, "Gave Birth"), (13, "k1", 14_000, "Slow")] {
            conn.execute(
                "UPDATE djmdContent SET KeyID = ?1, BPM = ?2, Title = ?3 WHERE ID = ?4",
                (key, bpm, title, track_id(index)),
            )
            .unwrap();
        }
        writer.set_tracks(&playlist_id(1), &[track_id(30), track_id(10), track_id(11), track_id(31), track_id(32)]).unwrap();
        Fixture { _dir: dir, location }
    }

    fn snapshot(f: &Fixture) -> Snapshot {
        Snapshot::load(Some(&f.location)).unwrap()
    }

    fn search(text: &str) -> Search {
        Search { text: Some(text.to_owned()), playlist: None, bpm_min: None, bpm_max: None, key: None, limit: 10 }
    }

    #[test]
    fn search_finds_by_folded_words_and_leaves_separators_out() {
        let f = fixture();
        let snap = snapshot(&f);
        let found = snap.search(&search("jales")).unwrap();
        assert_eq!(found.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Booyah Jale’s Edit"]);
        assert!(snap.search(&search("separatorbremsen")).unwrap().is_empty());

        let by_key = Search { key: Some("2A".to_owned()), ..search("") };
        let titles: Vec<String> = snap.search(&by_key).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles.len(), 3, "{titles:?}");
        assert!(!titles.contains(&"Gave Birth".to_owned()));
    }

    #[test]
    fn compatible_tracks_keep_to_the_wheel_and_the_tempo() {
        let f = fixture();
        let snap = snapshot(&f);
        let found = snap.compatible(&track_id(10), None, 3.0, 10).unwrap();
        let titles: Vec<&str> = found.matches.iter().map(|m| m.track.title.as_str()).collect();
        // Gave Birth is 5B against 2A, Slow is 140 BPM.
        assert_eq!(titles, ["Booyah Jale’s Edit"]);
        assert_eq!(found.matches[0].relation, Relation::Same);
    }

    #[test]
    fn a_playlist_reads_as_its_mini_sets() {
        let f = fixture();
        let snap = snapshot(&f);
        let view = snap.mini_sets("Playlist 1").unwrap();
        assert_eq!(view.blocks.len(), 1);
        assert_eq!(view.blocks[0].separator, "SEPARATORBREMSEN");
        assert_eq!(view.blocks[0].transitions[0].key, Some(Relation::Same));
        assert_eq!(view.empty_slots, ["SEPARATORBREMSEN 100", "SEPARATORBREMSEN 099"]);
        let listed = snap.playlists();
        let one = listed.iter().find(|p| p.id == playlist_id(1)).unwrap();
        assert_eq!((one.mini_sets.as_ref().unwrap().blocks, one.mini_sets.as_ref().unwrap().empty_slots), (1, 2));
    }

    #[test]
    fn the_preview_fills_the_slots_and_points_out_a_clash() {
        let f = fixture();
        let snap = snapshot(&f);
        let blocks = vec![vec![track_id(13), track_id(12)]];
        let preview = snap.preview(&Target::Playlist("Playlist 1".to_owned()), &blocks).unwrap();
        let block = &preview.blocks[0];
        assert_eq!(block.after, "SEPARATORBREMSEN 100");
        assert!(block.into_reserved_slot);
        assert_eq!(block.warnings.len(), 1, "{:?}", block.warnings);
        assert!(block.warnings[0].contains("key clash") && block.warnings[0].contains("tempo jump"));

        let new = snap.preview(&Target::New("Fresh".to_owned()), &blocks).unwrap();
        assert_eq!(new.blocks[0].after, "SEPARATORBREMSEN");
        assert!(snap.preview(&Target::New("playlist 1".to_owned()), &blocks).unwrap_err().contains("already at the top"));
        assert!(snap.preview(&Target::Playlist("Playlist 1".to_owned()), &[vec![track_id(10)]]).unwrap_err().contains("already in the playlist"));
        assert!(snap.preview(&Target::Playlist("Playlist 1".to_owned()), &[vec!["nope".to_owned()]]).unwrap_err().contains("No track"));
    }
}
