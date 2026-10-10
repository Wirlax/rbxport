//! Links to get tracks Ronan does not have yet: Deezer first, which deemix
//! downloads from, then `SoundCloud` when Deezer has nothing that is surely the
//! track. Deezer's public search needs no account; its quota is 50 requests
//! per 5 seconds, answered past that with error code 4
//! [REF developers.deezer.com/api/errors].

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rbl_db::catalog::CatalogTrack;
use serde::{Deserialize, Serialize};

use crate::library::{fold, Snapshot, TrackView};

const SEARCH: &str = "https://api.deezer.com/search";
/// Between two requests: the quota allows one per 100 ms.
const SPACING: Duration = Duration::from_millis(150);
/// How long a spent quota takes to come back.
const QUOTA_WAIT: Duration = Duration::from_secs(5);
const QUOTA_SPENT: i64 = 4;
/// Other versions shown for a track Deezer has no sure match for.
const CANDIDATES: usize = 3;

/// A cut that is the usual one rather than another version: a title with
/// it and one without are the same track.
const SAME_CUT: [&str; 9] = [
    "original mix",
    "original",
    "extended mix",
    "extended",
    "extended version",
    "album version",
    "explicit",
    "clean",
    "mixed",
];

/// A title as what the track is and which version of it, both folded.
#[derive(Debug, PartialEq, Eq)]
struct Title {
    base: String,
    versions: BTreeSet<String>,
}

/// `Bugatti (Pythius Remix)`, `Boulder [VIP]`, `Sinking - VIP Mix`: the base
/// and the versions in brackets or after the last ` - `. Featured artists and
/// the usual cut are not versions.
fn title(text: &str) -> Title {
    let mut base = String::new();
    let mut versions = BTreeSet::new();
    let mut depth = 0_u32;
    let mut group = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' => {
                if depth > 0 {
                    group.push(c);
                }
                depth += 1;
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    version(&group, &mut versions);
                    group.clear();
                } else {
                    group.push(c);
                }
            }
            _ if depth > 0 => group.push(c),
            _ => base.push(c),
        }
    }
    let base = match base.rsplit_once(" - ") {
        Some((head, tail)) => {
            version(tail, &mut versions);
            head.to_owned()
        }
        None => base,
    };
    Title { base: without_featuring(&fold(&base)), versions }
}

fn version(group: &str, versions: &mut BTreeSet<String>) {
    let folded = fold(group);
    let featuring = ["feat ", "ft ", "featuring ", "with "].iter().any(|w| folded.starts_with(w));
    if folded.is_empty() || featuring || SAME_CUT.contains(&folded.as_str()) || folded.contains("remaster") {
        return;
    }
    versions.insert(folded);
}

fn without_featuring(folded: &str) -> String {
    let words: Vec<&str> = folded.split(' ').collect();
    let end = words.iter().position(|w| matches!(*w, "feat" | "ft" | "featuring")).unwrap_or(words.len());
    words.get(..end).unwrap_or_default().join(" ")
}

/// Each artist a credit names, folded: `A/B`, `A, B`, `A & B`, `A x B`,
/// `A feat. B`, `A vs B`.
fn artists(credit: &str) -> Vec<String> {
    let lowered = format!(" {} ", credit.to_lowercase());
    let mut text = lowered;
    for joiner in [" feat. ", " feat ", " ft. ", " ft ", " featuring ", " x ", " vs. ", " vs ", " and ", " with "] {
        text = text.replace(joiner, "/");
    }
    text.split(['/', ',', '&', ';', '+']).map(fold).filter(|a| !a.is_empty()).collect()
}

/// Whether one of the wanted artists is credited, as artist, featured
/// artist or remixer: `haystack` is the credit and the title together.
fn credited(wanted: &[String], haystack: &str) -> bool {
    if wanted.is_empty() {
        return true;
    }
    let padded = format!(" {} ", fold(haystack));
    wanted.iter().any(|artist| padded.contains(&format!(" {artist} ")))
}

// ---------------------------------------------------------------- deezer

#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<Found>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Found {
    title: String,
    #[serde(default)]
    title_version: String,
    link: String,
    #[serde(default)]
    duration: u32,
    #[serde(default)]
    rank: u64,
    artist: Named,
    album: Album,
}

#[derive(Debug, Clone, Deserialize)]
struct Named {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Album {
    title: String,
}

impl Found {
    /// The title with its version, which Deezer sometimes keeps apart.
    fn full_title(&self) -> String {
        if self.title_version.is_empty() || self.title.contains(&self.title_version) {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.title_version)
        }
    }

    fn choice(&self, differs: Option<String>) -> Choice {
        Choice {
            link: self.link.clone(),
            title: self.full_title(),
            artist: self.artist.name.clone(),
            album: self.album.title.clone(),
            length: format!("{}:{:02}", self.duration / 60, self.duration % 60),
            differs,
        }
    }
}

/// Deezer's search, paced to its quota.
pub struct Deezer {
    agent: ureq::Agent,
    last: Option<Instant>,
}

impl Default for Deezer {
    fn default() -> Self {
        Self::new()
    }
}

impl Deezer {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
        Self { agent, last: None }
    }

    fn search(&mut self, query: &str) -> Result<Vec<Found>, String> {
        for attempt in 0..2 {
            if let Some(wait) = self.last.map(|last| SPACING.saturating_sub(last.elapsed())) {
                std::thread::sleep(wait);
            }
            self.last = Some(Instant::now());
            let url = format!("{SEARCH}?limit=25&q={}", utf8_percent_encode(query, NON_ALPHANUMERIC));
            let text = self
                .agent
                .get(&url)
                .call()
                .and_then(|mut response| response.body_mut().read_to_string())
                .map_err(|e| format!("Deezer could not be reached: {e}"))?;
            let page: Page = serde_json::from_str(&text).map_err(|e| format!("Deezer answered something unexpected: {e}"))?;
            match page.error {
                Some(error) if error.code == QUOTA_SPENT && attempt == 0 => std::thread::sleep(QUOTA_WAIT),
                Some(error) => return Err(format!("Deezer refused the search: {}", error.message)),
                None => return Ok(page.data),
            }
        }
        Err("Deezer's quota stayed spent.".to_owned())
    }
}

// ----------------------------------------------------------------- verdict

/// A track on Deezer, as a tool shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub link: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length: String,
    /// What makes it maybe not the track asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub differs: Option<String>,
}

/// One track of the list, and what was found for it.
#[derive(Debug, Serialize)]
pub struct Item {
    pub artist: String,
    pub title: String,
    /// `already_owned`, `deezer`, `uncertain`, `not_on_deezer` or `failed`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned: Option<TrackView>,
    /// Another version of it is in the library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_other_version: Option<TrackView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deezer: Option<Choice>,
    /// Deezer tracks that may be it, with what differs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Choice>,
    /// The last resort when no `SoundCloud` track page is found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soundcloud_search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub already_owned: usize,
    pub deezer: usize,
    pub uncertain: usize,
    pub not_on_deezer: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    /// One Deezer link per sure match, in the list's order: ready for deemix.
    pub deezer_links: Vec<String>,
    pub counts: Counts,
    pub items: Vec<Item>,
}

/// Looks up every wanted `(artist, title)`: the library first, then Deezer.
pub fn find(snapshot: &Snapshot, deezer: &mut Deezer, wanted: &[(String, String)]) -> Report {
    let mut report = Report { deezer_links: Vec::new(), counts: Counts::default(), items: Vec::with_capacity(wanted.len()) };
    for (artist, track) in wanted {
        let item = look_up(snapshot, deezer, artist, track);
        match item.status {
            "already_owned" => report.counts.already_owned += 1,
            "deezer" => report.counts.deezer += 1,
            "uncertain" => report.counts.uncertain += 1,
            "not_on_deezer" => report.counts.not_on_deezer += 1,
            _ => report.counts.failed += 1,
        }
        if let Some(choice) = item.deezer.as_ref().filter(|c| !report.deezer_links.contains(&c.link)) {
            report.deezer_links.push(choice.link.clone());
        }
        report.items.push(item);
    }
    report
}

fn look_up(snapshot: &Snapshot, deezer: &mut Deezer, artist: &str, track: &str) -> Item {
    let mut item = Item {
        artist: artist.to_owned(),
        title: track.to_owned(),
        status: "not_on_deezer",
        owned: None,
        owned_other_version: None,
        deezer: None,
        candidates: Vec::new(),
        soundcloud_search: None,
        error: None,
    };
    let (owned, other) = owned(snapshot.real_tracks(), artist, track);
    if let Some(owned) = owned {
        item.status = "already_owned";
        item.owned = Some(snapshot.view(owned));
        return item;
    }
    item.owned_other_version = other.map(|t| snapshot.view(t));

    let mut found: Vec<Found> = Vec::new();
    let mut queries = Vec::new();
    if !artist.trim().is_empty() {
        queries.push(format!("artist:\"{}\" track:\"{}\"", artist.trim(), track.trim()));
    }
    queries.push(format!("{} {}", artist.trim(), title(track).base).trim().to_owned());
    for query in queries {
        match deezer.search(&query) {
            Ok(page) => {
                let fresh: Vec<Found> = page.into_iter().filter(|f| !found_has(&found, f)).collect();
                found.extend(fresh);
            }
            Err(error) => {
                item.status = "failed";
                item.error = Some(error);
                return item;
            }
        }
        if verdict(artist, track, &found).0.is_some() {
            break;
        }
    }
    let (sure, maybe) = verdict(artist, track, &found);
    item.status = match (&sure, maybe.is_empty()) {
        (Some(_), _) => "deezer",
        (None, false) => "uncertain",
        (None, true) => "not_on_deezer",
    };
    if sure.is_none() {
        item.soundcloud_search = Some(soundcloud_search(artist, track));
    }
    item.deezer = sure;
    item.candidates = maybe;
    item
}

fn found_has(found: &[Found], candidate: &Found) -> bool {
    found.iter().any(|f| f.link == candidate.link)
}

/// The sure match among `found`, if any, and the ones that may be it.
///
/// Sure: the same title, the same versions, a wanted artist credited. Among
/// several, the extended cut when no version was asked for — a DJ mixes the
/// long one — then the most played. Maybe: the same title with another
/// version or other artists.
fn verdict(artist: &str, track: &str, found: &[Found]) -> (Option<Choice>, Vec<Choice>) {
    let wanted = title(track);
    let names = artists(artist);
    let mut sure: Vec<&Found> = Vec::new();
    let mut maybe: Vec<(&Found, String)> = Vec::new();
    for candidate in found {
        let full = candidate.full_title();
        let theirs = title(&full);
        if theirs.base != wanted.base {
            continue;
        }
        let by = credited(&names, &format!("{} {full}", candidate.artist.name));
        let mut differs = Vec::new();
        if theirs.versions != wanted.versions {
            let shown = |v: &BTreeSet<String>| if v.is_empty() { "none".to_owned() } else { v.iter().cloned().collect::<Vec<_>>().join(", ") };
            differs.push(format!("version {} instead of {}", shown(&theirs.versions), shown(&wanted.versions)));
        }
        if !by {
            differs.push(format!("by {}", candidate.artist.name));
        }
        if differs.is_empty() {
            sure.push(candidate);
        } else {
            maybe.push((candidate, differs.join("; ")));
        }
    }
    let extended = |f: &Found| wanted.versions.is_empty() && fold(&f.full_title()).contains("extended");
    sure.sort_by(|a, b| extended(b).cmp(&extended(a)).then(b.rank.cmp(&a.rank)));
    maybe.sort_by_key(|(f, _)| std::cmp::Reverse(f.rank));
    (
        sure.first().map(|f| f.choice(None)),
        maybe.into_iter().take(CANDIDATES).map(|(f, differs)| f.choice(Some(differs))).collect(),
    )
}

/// The library's copy of the track, or failing that another version of it.
/// A track with no artist in the library counts on its title alone: some came
/// in untagged.
fn owned<'a>(
    tracks: impl Iterator<Item = &'a CatalogTrack>,
    artist: &str,
    track: &str,
) -> (Option<&'a CatalogTrack>, Option<&'a CatalogTrack>) {
    let wanted = title(track);
    let names = artists(artist);
    let mut other = None;
    for candidate in tracks {
        let theirs = title(&candidate.title);
        if theirs.base != wanted.base {
            continue;
        }
        let by = candidate.artist.trim().is_empty()
            || credited(&names, &format!("{} {} {}", candidate.artist.replace('/', " "), candidate.remixer, candidate.title));
        if !by {
            continue;
        }
        if theirs.versions == wanted.versions {
            return (Some(candidate), None);
        }
        other.get_or_insert(candidate);
    }
    (None, other)
}

fn soundcloud_search(artist: &str, track: &str) -> String {
    let query = format!("{} {}", artist.trim(), track.trim());
    format!("https://soundcloud.com/search/sounds?q={}", utf8_percent_encode(query.trim(), NON_ALPHANUMERIC))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn found(title: &str, version: &str, artist: &str, rank: u64) -> Found {
        Found {
            title: title.to_owned(),
            title_version: version.to_owned(),
            link: format!("https://www.deezer.com/track/{}", fold(&format!("{title} {version} {artist}")).replace(' ', "-")),
            duration: 200,
            rank,
            artist: Named { name: artist.to_owned() },
            album: Album { title: "Album".to_owned() },
        }
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_title_splits_into_what_it_is_and_which_version() {
        assert_eq!(title("Bugatti (Pythius Remix)"), Title { base: "bugatti".into(), versions: set(&["pythius remix"]) });
        assert_eq!(title("Sinking - VIP Mix"), Title { base: "sinking".into(), versions: set(&["vip mix"]) });
        assert_eq!(title("Boulder [Extended Mix]"), Title { base: "boulder".into(), versions: set(&[]) });
        assert_eq!(title("Gave Birth (feat. Someone) (2024 Remaster)"), Title { base: "gave birth".into(), versions: set(&[]) });
        assert_eq!(title("No Surrender feat. Pirapus"), Title { base: "no surrender".into(), versions: set(&[]) });
        assert_eq!(title("Booyah Jale’s Edit").base, "booyah jales edit");
    }

    #[test]
    fn a_credit_splits_into_its_artists() {
        assert_eq!(artists("Modestep/H808/Pythius"), ["modestep", "h808", "pythius"]);
        assert_eq!(artists("SOTA & Pirapus"), ["sota", "pirapus"]);
        assert_eq!(artists("Sub Focus x Dimension feat. Kele"), ["sub focus", "dimension", "kele"]);
        assert_eq!(artists(""), Vec::<String>::new());
    }

    #[test]
    fn the_sure_match_prefers_the_extended_cut() {
        let found = vec![
            found("Sinking", "", "Enei", 900),
            found("Sinking (Extended Mix)", "", "Enei", 100),
            found("Sinking (VIP)", "", "Enei", 500),
        ];
        let (sure, maybe) = verdict("Enei", "Sinking", &found);
        assert_eq!(sure.unwrap().title, "Sinking (Extended Mix)");
        assert_eq!(maybe.len(), 1);
        assert_eq!(maybe[0].differs.as_deref(), Some("version vip instead of none"));
    }

    #[test]
    fn another_version_or_artist_is_only_a_maybe() {
        let found = vec![found("Boulder", "", "Brigsy", 10), found("Boulder (VIP)", "", "Someone Else", 5)];
        let (sure, maybe) = verdict("Brigsy", "Boulder (VIP)", &found);
        assert!(sure.is_none());
        assert_eq!(maybe[0].differs.as_deref(), Some("version none instead of vip"));
        assert_eq!(maybe[1].differs.as_deref(), Some("by Someone Else"));
        assert_eq!(verdict("Brigsy", "Nothing Like It", &found).1.len(), 0);
    }

    #[test]
    fn a_remixer_credited_in_the_title_counts_as_the_artist() {
        let found = vec![found("Bugatti", "(Pythius Remix)", "Modestep", 10)];
        let (sure, _) = verdict("Pythius", "Bugatti (Pythius Remix)", &found);
        assert_eq!(sure.unwrap().title, "Bugatti (Pythius Remix)");
    }

    #[test]
    fn the_library_copy_or_another_version_is_found() {
        let track = |title: &str, artist: &str| CatalogTrack { title: title.to_owned(), artist: artist.to_owned(), ..CatalogTrack::default() };
        let library = [track("Boulder", "Brigsy"), track("Booyah Jale’s Edit", ""), track("Bugatti (Pythius Remix)", "Modestep/H808/Pythius")];
        let (owned_one, _) = owned(library.iter(), "Brigsy", "Boulder");
        assert_eq!(owned_one.unwrap().title, "Boulder");
        let (none, other) = owned(library.iter(), "Brigsy", "Boulder (VIP)");
        assert!(none.is_none());
        assert_eq!(other.unwrap().title, "Boulder");
        assert!(owned(library.iter(), "Jale", "Booyah Jale's Edit").0.is_some(), "untagged: the title alone");
        assert!(owned(library.iter(), "H808", "Bugatti (Pythius Remix)").0.is_some());
        assert!(owned(library.iter(), "Someone", "Boulder").0.is_none());
    }

    #[test]
    fn deezer_pages_and_errors_parse() {
        let page: Page = serde_json::from_str(
            r#"{"data":[{"id":1,"title":"One More Time","title_version":"","link":"https://www.deezer.com/track/1","duration":320,"rank":9,"artist":{"name":"Daft Punk"},"album":{"title":"Discovery"}}],"total":1}"#,
        )
        .unwrap();
        assert_eq!(page.data[0].choice(None).length, "5:20");
        let quota: Page = serde_json::from_str(r#"{"error":{"type":"Exception","message":"Quota limit exceeded","code":4}}"#).unwrap();
        assert_eq!(quota.error.unwrap().code, QUOTA_SPENT);
    }

    #[test]
    fn the_soundcloud_search_is_one_encoded_query() {
        assert_eq!(soundcloud_search("Brigsy", "Boulder (VIP)"), "https://soundcloud.com/search/sounds?q=Brigsy%20Boulder%20%28VIP%29");
    }
}
