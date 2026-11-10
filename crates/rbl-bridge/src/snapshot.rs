//! The library as the API serves it: read once, read-only, into plain data
//! that needs no database handle, so requests never touch `SQLCipher` and a
//! rebuild never blocks the ones in flight.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rbl_core::musickey::{self, Key, Mode};
use rbl_db::catalog::{self, CatalogCue, CatalogNode};
use rbl_db::mini_sets::SEPARATOR_TITLE;
use rbl_db::write::{ATTRIBUTE_FOLDER, ROOT};
use rbl_db::{Library, LibraryLocation, OpenMode};
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

/// Colours of a memory cue's named palette, indexed by `djmdCue.Color`.
const MEMORY_CSS: [&str; 8] = ["#E778F1", "#E33122", "#EBA44A", "#F4E458", "#66DD42", "#56BDF3", "#204FEF", "#8B1EEF"];

/// A preview starts at a cue only if it is at least this far into the track.
const PREVIEW_FROM_MS: i64 = 15_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub kind: &'static str,
    pub slot: Option<String>,
    pub ms: i64,
    /// Length of the loop, when the cue is one.
    pub loop_ms: Option<i64>,
    pub comment: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackJson {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub remixer: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub camelot: Option<String>,
    pub duration_sec: i64,
    pub rating: i64,
    pub comment: Option<String>,
    pub bitrate: Option<i64>,
    pub format: Option<String>,
    pub date_added: Option<String>,
    pub available: bool,
    pub preview_ms: i64,
    pub playlist_ids: Vec<String>,
    pub cues: Vec<Cue>,
}

pub struct TrackEntry {
    pub json: TrackJson,
    /// Where the file is on this machine, from the library only.
    pub path: PathBuf,
    cues: Vec<Cue>,
    /// Title, artist and remixer folded for searching.
    haystack: String,
    title_fold: String,
}

impl TrackEntry {
    /// The track with its cues filled in.
    pub fn detailed(&self) -> TrackJson {
        TrackJson { cues: self.cues.clone(), ..self.json.clone() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniSets {
    pub blocks: usize,
    pub empty_slots: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistJson {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub kind: &'static str,
    pub track_count: usize,
    pub mini_sets: Option<MiniSets>,
}

/// A playlist row: a track (by index into the snapshot) or a separator.
pub enum Row<'a> {
    Track(&'a TrackEntry),
    Separator(&'a str),
}

pub struct Snapshot {
    tracks: Vec<TrackEntry>,
    by_id: HashMap<String, usize>,
    playlists: Vec<PlaylistJson>,
    playlist_index: HashMap<String, usize>,
    /// Each playlist's rows in order: a track index, or a separator title.
    rows: HashMap<String, Vec<Member>>,
    /// Newest modification time of the database files when this was read.
    pub stamp: Stamp,
}

enum Member {
    Track(usize),
    Separator(String),
}

/// What changes when rekordbox writes: the database file and its journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stamp {
    db: Option<SystemTime>,
    wal: Option<SystemTime>,
}

impl Stamp {
    pub fn of(master_db: &Path) -> Self {
        let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut wal = master_db.as_os_str().to_owned();
        wal.push("-wal");
        Self { db: mtime(master_db), wal: mtime(Path::new(&wal)) }
    }

    /// The most recent of the two, when either exists.
    pub fn updated_at(self) -> Option<SystemTime> {
        self.db.into_iter().chain(self.wal).max()
    }
}

impl Snapshot {
    /// Reads the whole library through a read-only handle.
    pub fn load(location: &LibraryLocation) -> Result<Self, String> {
        let stamp = Stamp::of(&location.master_db);
        let db = Library::open(location.clone(), OpenMode::ReadOnly).map_err(|e| e.to_string())?;
        Self::read(&db, stamp).map_err(|e| e.to_string())
    }

    fn read(db: &Library, stamp: Stamp) -> rbl_db::Result<Self> {
        let conn = db.connection();
        let catalog_tracks = catalog::tracks(conn)?;
        let nodes = tree_order(&catalog::nodes(conn)?);
        let files: HashMap<String, catalog::CatalogFile> = catalog::files(conn)?.into_iter().map(|f| (f.id.clone(), f)).collect();
        let mut cues_of: HashMap<String, Vec<CatalogCue>> = HashMap::new();
        for cue in catalog::cues(conn)? {
            cues_of.entry(cue.track.clone()).or_default().push(cue);
        }
        let paths = db.track_paths();

        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (playlist, content) in catalog::memberships(conn)? {
            members.entry(playlist).or_default().push(content);
        }

        let mut tracks = Vec::new();
        let mut by_id = HashMap::new();
        let mut separators: HashMap<String, String> = HashMap::new();
        for t in catalog_tracks {
            if is_separator(&t.title) {
                separators.insert(t.id, t.title.trim().to_owned());
                continue;
            }
            let file = files.get(&t.id);
            let path = file.map(|f| PathBuf::from(paths.resolve(&f.stored))).unwrap_or_default();
            let cues: Vec<Cue> = cues_of.get(&t.id).map(|list| list.iter().filter_map(cue).collect()).unwrap_or_default();
            let key = musickey::parse(&t.key);
            let duration_ms = t.length * 1000;
            let json = TrackJson {
                id: t.id.clone(),
                title: t.title.clone(),
                artist: some(&t.artist),
                remixer: some(&t.remixer),
                album: some(&t.album),
                genre: some(&t.genre),
                bpm: (t.bpm_x100 > 0).then(|| bpm_of(t.bpm_x100)),
                key: some(&t.key),
                camelot: key.map(camelot_code),
                duration_sec: t.length,
                rating: t.rating.clamp(0, 5),
                comment: some(&t.comment),
                bitrate: file.map(|f| f.bitrate).filter(|&b| b > 0),
                format: path.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| !e.is_empty()),
                date_added: file.and_then(|f| date_of(&f.date_created)),
                available: !path.as_os_str().is_empty() && path.is_file(),
                preview_ms: preview_ms(&cues, duration_ms),
                playlist_ids: Vec::new(),
                cues: Vec::new(),
            };
            by_id.insert(t.id.clone(), tracks.len());
            tracks.push(TrackEntry {
                haystack: haystack(&format!("{} {} {}", t.title, t.artist, t.remixer)),
                title_fold: fold(&t.title),
                json,
                path,
                cues,
            });
        }

        let mut playlists = Vec::with_capacity(nodes.len());
        let mut playlist_index = HashMap::new();
        let mut rows = HashMap::new();
        for node in &nodes {
            let mut list = Vec::new();
            let (mut blocks, mut empty, mut seen) = (0, 0, false);
            let mut open_block_has_tracks = false;
            for content in members.get(&node.id).into_iter().flatten() {
                if let Some(title) = separators.get(content) {
                    if seen && !open_block_has_tracks {
                        empty += 1;
                    } else if seen {
                        blocks += 1;
                    }
                    seen = true;
                    open_block_has_tracks = false;
                    list.push(Member::Separator(title.clone()));
                } else if let Some(&index) = by_id.get(content) {
                    open_block_has_tracks = true;
                    list.push(Member::Track(index));
                    if let Some(track) = tracks.get_mut(index) {
                        if track.json.playlist_ids.last() != Some(&node.id) {
                            track.json.playlist_ids.push(node.id.clone());
                        }
                    }
                }
            }
            if seen {
                if open_block_has_tracks {
                    blocks += 1;
                } else {
                    empty += 1;
                }
            }
            let track_count = list.iter().filter(|m| matches!(m, Member::Track(_))).count();
            playlist_index.insert(node.id.clone(), playlists.len());
            playlists.push(PlaylistJson {
                id: node.id.clone(),
                name: node.name.clone(),
                parent_id: (node.parent != ROOT).then(|| node.parent.clone()),
                kind: if node.attribute == ATTRIBUTE_FOLDER { "folder" } else { "playlist" },
                track_count,
                mini_sets: seen.then_some(MiniSets { blocks, empty_slots: empty }),
            });
            rows.insert(node.id.clone(), list);
        }
        Ok(Self { tracks, by_id, playlists, playlist_index, rows, stamp })
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    pub fn playlists(&self) -> &[PlaylistJson] {
        &self.playlists
    }

    pub fn track(&self, id: &str) -> Option<&TrackEntry> {
        self.by_id.get(id).and_then(|&i| self.tracks.get(i))
    }

    pub fn playlist(&self, id: &str) -> Option<&PlaylistJson> {
        self.playlist_index.get(id).and_then(|&i| self.playlists.get(i))
    }

    /// A playlist's rows in order; `None` for an unknown id.
    pub fn playlist_rows(&self, id: &str) -> Option<Vec<Row<'_>>> {
        let rows = self.rows.get(id)?;
        Some(
            rows.iter()
                .filter_map(|m| match m {
                    Member::Track(i) => self.tracks.get(*i).map(Row::Track),
                    Member::Separator(title) => Some(Row::Separator(title)),
                })
                .collect(),
        )
    }

    /// Tracks whose title, artist or remixer hold every word of `query`,
    /// folded for case and accents, by title. Returns the total and the page.
    pub fn search(&self, query: &str, offset: usize, limit: usize) -> (usize, Vec<TrackJson>) {
        let words: Vec<String> = fold(query).split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect();
        let mut found: Vec<&TrackEntry> =
            self.tracks.iter().filter(|t| words.iter().all(|w| t.haystack.contains(w.as_str()))).collect();
        found.sort_by(|a, b| a.title_fold.cmp(&b.title_fold).then_with(|| a.json.id.cmp(&b.json.id)));
        let total = found.len();
        (total, found.into_iter().skip(offset).take(limit).map(|t| t.json.clone()).collect())
    }
}

fn is_separator(title: &str) -> bool {
    title.trim().starts_with(SEPARATOR_TITLE)
}

fn some(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn bpm_of(x100: i64) -> f64 {
    f64::from(i32::try_from(x100).unwrap_or(0)) / 100.0
}

/// `YYYY-MM-DD` from whatever the column holds, or `None`.
fn date_of(text: &str) -> Option<String> {
    let day = text.trim().get(..10)?;
    let bytes = day.as_bytes();
    let shaped = bytes.iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() });
    shaped.then(|| day.to_owned())
}

/// Folders before their contents, siblings in order: the tree as rekordbox
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

/// A stored cue as served; `None` for a kind that is neither memory nor hot.
fn cue(c: &CatalogCue) -> Option<Cue> {
    let slot = match c.kind {
        0 => None,
        1..=3 => char::from_u32(u32::from(b'A') + u32::try_from(c.kind - 1).ok()?),
        5..=17 => char::from_u32(u32::from(b'A') + u32::try_from(c.kind - 2).ok()?),
        _ => return None,
    };
    let memory = c.kind == 0;
    let color = if memory {
        usize::try_from(c.color).ok().and_then(|i| MEMORY_CSS.get(i)).map(|css| (*css).to_owned())
    } else {
        u8::try_from(c.color_table_index)
            .ok()
            .filter(|&i| i != 0)
            .and_then(rbl_anlz::cue_colour_drawn)
            .map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}"))
    };
    Some(Cue {
        kind: if memory { "memory" } else { "hot" },
        slot: slot.map(String::from),
        ms: c.in_msec,
        loop_ms: (c.out_msec > c.in_msec).then(|| c.out_msec - c.in_msec),
        comment: some(&c.comment),
        color,
    })
}

/// Where the quick preview starts: the first hot cue at 15 s or later, else
/// the first memory cue at 15 s or later, else a quarter of the track.
fn preview_ms(cues: &[Cue], duration_ms: i64) -> i64 {
    let first = |kind: &str| cues.iter().filter(|c| c.kind == kind && c.ms >= PREVIEW_FROM_MS).map(|c| c.ms).min();
    first("hot").or_else(|| first("memory")).unwrap_or(duration_ms / 4)
}

/// The Camelot wheel's minor keys, 1A to 12A, as pitch classes.
const CAMELOT_MINOR: [u8; 12] = [8, 3, 10, 5, 0, 7, 2, 9, 4, 11, 6, 1];

pub fn camelot_code(key: Key) -> String {
    let minor = key.mode == Mode::Minor;
    // A major key shares its number with the minor three semitones below.
    let minor_pitch = if minor { key.pitch % 12 } else { (key.pitch + 9) % 12 };
    let number = CAMELOT_MINOR.iter().position(|&p| p == minor_pitch).map_or(0, |i| i + 1);
    format!("{number}{}", if minor { 'A' } else { 'B' })
}

/// What a search looks in: the text folded, and folded again with its dots
/// closed up, so `A.M.C` answers to both `a m c` and `amc`.
fn haystack(text: &str) -> String {
    format!("{} {}", fold(text), fold(&text.replace('.', "")))
}

/// Lower case, accents and apostrophes gone, anything else that is not a
/// letter or digit a single space.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn camelot_codes_follow_the_wheel() {
        assert_eq!(camelot_code(musickey::parse("Ebm").unwrap()), "2A");
        assert_eq!(camelot_code(musickey::parse("Eb").unwrap()), "5B");
        assert_eq!(camelot_code(musickey::parse("Dm").unwrap()), "7A");
    }

    #[test]
    fn folding_ignores_case_accents_and_apostrophes() {
        assert_eq!(fold("Booyah Jale’s Edit"), "booyah jales edit");
        assert_eq!(fold("  Beyoncé  — Déjà Vu "), "beyonce deja vu");
    }

    #[test]
    fn dotted_names_are_found_with_or_without_the_dots() {
        let text = haystack("I See You A.M.C");
        for word in ["amc", "a", "m", "see you"] {
            assert!(text.contains(word), "{word}");
        }
    }

    #[test]
    fn dates_keep_only_the_day() {
        assert_eq!(date_of("2025-03-04 10:00:00.123 +00:00").as_deref(), Some("2025-03-04"));
        assert_eq!(date_of(""), None);
        assert_eq!(date_of("yesterday!"), None);
    }

    fn cue_at(kind: &'static str, ms: i64) -> Cue {
        Cue { kind, slot: None, ms, loop_ms: None, comment: None, color: None }
    }

    #[test]
    fn the_preview_prefers_hot_then_memory_then_a_quarter() {
        let hot = [cue_at("memory", 20_000), cue_at("hot", 16_000), cue_at("hot", 5_000)];
        assert_eq!(preview_ms(&hot, 200_000), 16_000);
        let memory = [cue_at("memory", 30_000), cue_at("memory", 2_000), cue_at("hot", 14_999)];
        assert_eq!(preview_ms(&memory, 200_000), 30_000);
        assert_eq!(preview_ms(&[cue_at("hot", 1_000)], 200_000), 50_000);
    }
}
