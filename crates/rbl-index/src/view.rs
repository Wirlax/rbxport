//! Views: an ordered, filtered list of rows.
//!
//! A view is just a `Vec<Row>`. Sorting compares precomputed rank integers
//! rather than strings, and searching scans one packed folded haystack, so both
//! stay well inside the budgets on a 38k-track library.

use crate::{filter::TrackFilter, strings::fold, Library, Row};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortColumn {
    TrackNo,
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Key,
    Bpm,
    Duration,
    Rating,
    DateAdded,
    ReleaseDate,
    /// The key column round the Camelot wheel, for the alphanumeric display:
    /// rekordbox sorts the column by what it shows.
    KeyCamelot,
}

impl SortColumn {
    pub(crate) const ALL: [SortColumn; 13] = [
        SortColumn::TrackNo, SortColumn::Title, SortColumn::Artist, SortColumn::Album,
        SortColumn::Genre, SortColumn::Label, SortColumn::Key, SortColumn::Bpm,
        SortColumn::Duration, SortColumn::Rating, SortColumn::DateAdded, SortColumn::ReleaseDate,
        SortColumn::KeyCamelot,
    ];

    pub(crate) fn rank_slot(self) -> usize {
        match self {
            SortColumn::TrackNo => 0,
            SortColumn::Title => 1,
            SortColumn::Artist => 2,
            SortColumn::Album => 3,
            SortColumn::Genre => 4,
            SortColumn::Label => 5,
            SortColumn::Key => 6,
            SortColumn::Bpm => 7,
            SortColumn::Duration => 8,
            SortColumn::Rating => 9,
            SortColumn::DateAdded => 10,
            SortColumn::ReleaseDate => 11,
            SortColumn::KeyCamelot => 12,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSource {
    Collection,
    /// Index into `Library::playlists`, not a rekordbox id.
    Playlist(usize),
    /// Index into `Library::histories`. A session, or a folder of them —
    /// a folder has no members of its own, so it opens empty.
    History(usize),
    /// Index into `Library::playlists` of an intelligent playlist: the rows
    /// are whatever its rule admits at the moment it is opened.
    SmartPlaylist(usize),
    /// rekordbox's Related Tracks: the tracks that go with one track under a
    /// criterion. A `track` past the end of the library — no track loaded —
    /// opens empty.
    Related { track: Row, criterion: RelatedCriterion },
    /// rekordbox's Tag List, in its own order.
    TagList,
}

/// The Related Tracks section's criteria, the three rekordbox's Export
/// mode lists [DOC: the rekordbox manual's Related Tracks; the panel itself
/// has no capture here].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelatedCriterion {
    /// `BPM + KEY`: within 6 % of the track's BPM [ASSUME] and in its key or
    /// a key beside it on the wheel (Relative Key 1).
    BpmAndKey,
    /// `Same genre in 30 days`: the track's genre, added in the last thirty
    /// days [ASSUME: what the thirty days count].
    SameGenreRecent,
    /// `Same artist`.
    SameArtist,
    /// rekordbox's Track Suggestion: what was played after this track in
    /// the histories, the most often first, then the most recently, and
    /// with no history of the track, what goes with it by BPM and key.
    /// rekordbox 7.2.11's own panel (captured 2026-09-18) is titled "Era",
    /// takes its track from the list, the master or player A, and scopes
    /// to the collection; what it ranks by is not shown and not
    /// documented, so this is a stand-in, not a copy.
    Suggestion,
}

#[derive(Debug, Clone)]
pub struct ViewSpec {
    pub source: TrackSource,
    pub sort: SortColumn,
    pub descending: bool,
    pub query: String,
    /// The track filter bar's picks. Default is no constraint.
    pub filter: TrackFilter,
}

#[derive(Debug)]
pub struct View {
    pub rows: Vec<Row>,
}

impl View {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// A window of rows, clamped to what exists.
    pub fn window(&self, offset: usize, len: usize) -> &[Row] {
        let start = offset.min(self.rows.len());
        let end = start.saturating_add(len).min(self.rows.len());
        self.rows.get(start..end).unwrap_or(&[])
    }
}

impl Library {
    /// Builds a view. This is the only place ordering is decided.
    pub fn open_view(&self, spec: &ViewSpec) -> View {
        let mut rows: Vec<Row> = self.source_rows(&spec.source);

        let query = fold(spec.query.trim());
        if !query.is_empty() {
            rows.retain(|&r| self.row_matches(r, &query));
        }

        // The filter bar, in the same pass as the search: a handful of integer
        // compares per row against masks built once, so a ticked column costs
        // about what a one-letter query does.
        if !spec.filter.is_empty() {
            let compiled = spec.filter.compile();
            rows.retain(|&r| compiled.matches(self, r));
        }

        // `TrackNo` is not a column to sort by — it *is* the view's own order:
        // the collection's row order, or a playlist's membership order. Ranking
        // by it would reorder a playlist into collection order, which is
        // exactly what turning sorting off must not do.
        if spec.sort == SortColumn::TrackNo {
            if spec.descending {
                rows.reverse();
            }
        } else {
            self.sort_rows(&mut rows, spec.sort, spec.descending);
        }
        View { rows }
    }

    /// Narrows an existing view. Typing another character only has to filter the
    /// previous match set, not the whole library.
    pub fn refine(&self, previous: &View, query: &str) -> View {
        let folded = fold(query.trim());
        if folded.is_empty() {
            // perf-ok: clearing the query copies at most 40k u32 (~160 KB, tens
            // of microseconds) and only on the keystroke that empties the box.
            return View { rows: previous.rows.clone() };
        }
        View {
            rows: previous.rows.iter().copied().filter(|&r| self.row_matches(r, &folded)).collect(),
        }
    }

    pub(crate) fn row_matches(&self, row: Row, folded_query: &str) -> bool {
        let hay = self.search.get(row as usize);
        // Every token must appear, so "artbat 128" narrows as a user expects.
        folded_query.split_whitespace().all(|token| {
            memchr::memmem::find(hay.as_bytes(), token.as_bytes()).is_some()
        })
    }

    /// Orders rows by a column's precomputed ranks. Public for the link
    /// export, whose menus sort scopes the views do not have (an artist's
    /// tracks, a key's) with the same ranks the browser uses.
    pub fn sort_rows(&self, rows: &mut [Row], column: SortColumn, descending: bool) {
        let Some(rank) = self.ranks.get(column.rank_slot()) else { return };
        if descending {
            rows.sort_unstable_by_key(|&r| std::cmp::Reverse(rank.get(r as usize).copied().unwrap_or(0)));
        } else {
            rows.sort_unstable_by_key(|&r| rank.get(r as usize).copied().unwrap_or(0));
        }
    }

    /// Ids of rows between two view positions, inclusive. Used for shift-click
    /// across rows the frontend has never fetched.
    pub fn ids_in_range(&self, view: &View, from: usize, to: usize) -> Vec<u64> {
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        let hi = hi.min(view.rows.len().saturating_sub(1));
        view.rows
            .get(lo..=hi)
            .unwrap_or(&[])
            .iter()
            .filter_map(|&r| self.ids.get(r as usize).copied())
            .collect()
    }

    /// Builds the per-column rank arrays. Called once at load.
    pub(crate) fn build_ranks(&mut self) {
        let n = self.count;
        let mut ranks = Vec::with_capacity(SortColumn::ALL.len());
        for column in SortColumn::ALL {
            let mut order: Vec<Row> = (0..u32::try_from(n).unwrap_or(u32::MAX)).collect();
            // Ties break on row order so a sort is reproducible.
            match column {
                SortColumn::TrackNo => {}
                SortColumn::Bpm => order.sort_by_key(|&r| self.bpm_x100.get(r as usize).copied().unwrap_or(0)),
                SortColumn::Duration => order.sort_by_key(|&r| self.length_sec.get(r as usize).copied().unwrap_or(0)),
                SortColumn::Rating => order.sort_by_key(|&r| self.rating.get(r as usize).copied().unwrap_or(0)),
                SortColumn::Title => order.sort_by(|&a, &b| self.title_folded.get(a as usize).cmp(self.title_folded.get(b as usize))),
                SortColumn::Artist => order.sort_by(|&a, &b| Self::folded_lookup(&self.artists, &self.artist, a).cmp(Self::folded_lookup(&self.artists, &self.artist, b))),
                SortColumn::Album => order.sort_by(|&a, &b| Self::folded_lookup(&self.albums, &self.album, a).cmp(Self::folded_lookup(&self.albums, &self.album, b))),
                SortColumn::Genre => order.sort_by(|&a, &b| Self::folded_lookup(&self.genres, &self.genre, a).cmp(Self::folded_lookup(&self.genres, &self.genre, b))),
                SortColumn::Label => order.sort_by(|&a, &b| Self::folded_lookup(&self.labels, &self.label, a).cmp(Self::folded_lookup(&self.labels, &self.label, b))),
                // By the key's own rule, not the fold: the fold drops `#`,
                // which put F and F# on top of each other.
                SortColumn::Key => order.sort_by(|&a, &b| crate::key::cmp_names(self.key_name(a), self.key_name(b))),
                SortColumn::KeyCamelot => order.sort_by(|&a, &b| {
                    crate::key::camelot_rank(self.key_name(a))
                        .cmp(&crate::key::camelot_rank(self.key_name(b)))
                        .then_with(|| crate::key::cmp_names(self.key_name(a), self.key_name(b)))
                }),
                SortColumn::DateAdded => order.sort_by(|&a, &b| self.date_added.get(a as usize).cmp(self.date_added.get(b as usize))),
                SortColumn::ReleaseDate => order.sort_by(|&a, &b| self.release_date.get(a as usize).cmp(self.release_date.get(b as usize))),
            }
            let mut rank = vec![0_u32; n];
            for (position, &row) in order.iter().enumerate() {
                if let Some(slot) = rank.get_mut(row as usize) {
                    *slot = u32::try_from(position).unwrap_or(u32::MAX);
                }
            }

            ranks.push(rank);
        }
        self.ranks = ranks;
    }

    /// Free function: it reads only its arguments, not `self`.
    fn folded_lookup<'a>(interner: &'a crate::strings::Interner, ids: &[u32], row: Row) -> &'a str {
        interner.folded(ids.get(row as usize).copied().unwrap_or(crate::NO_ID))
    }

    /// Builds the folded search haystack. Called once at load.
    pub(crate) fn build_search(&mut self) {
        let mut search = crate::strings::StrColumn::with_capacity(self.count, self.count * 64);
        for row in 0..self.count {
            let mut hay = String::with_capacity(96);
            hay.push_str(self.title_folded.get(row));
            hay.push(' ');
            hay.push_str(self.artists.folded(self.artist.get(row).copied().unwrap_or(crate::NO_ID)));
            hay.push(' ');
            hay.push_str(self.albums.folded(self.album.get(row).copied().unwrap_or(crate::NO_ID)));
            hay.push(' ');
            hay.push_str(&fold(self.comment.get(row)));
            search.push(&hay);
        }
        self.search = search;
    }
}
