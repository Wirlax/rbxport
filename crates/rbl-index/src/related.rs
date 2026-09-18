//! rekordbox's Related Tracks: the tracks that go with the one on the
//! player, under one of the section's criteria.
//!
//! Each criterion is a pass over the library's columns — a handful of
//! integer compares a row, the same cost as the filter bar — so the section
//! opens as fast as a playlist does. The track itself is left out: it is
//! related to nothing but itself.

use crate::key::camelot_rank;
use crate::smart::{today, Date};
use crate::view::RelatedCriterion;
use crate::{Library, Row};

/// How far either side of the track's BPM `BPM + KEY` reaches, in
/// hundredths of a percent. [ASSUME] six percent: rekordbox's own default is
/// not recorded here.
const BPM_TOLERANCE_X100: u64 = 600;
/// How many days `Same genre in 30 days` looks back.
const RECENT_DAYS: i64 = 30;

/// Whether two wheel positions are the same key, its relative, or the key
/// either side of it: `1A` goes with `1A`, `1B`, `12A` and `2A`.
fn keys_go_together(a: u32, b: u32) -> bool {
    if a == u32::MAX || b == u32::MAX {
        return false;
    }
    if a == b || a ^ 1 == b {
        return true;
    }
    // Same letter, one step round the wheel of twelve either way.
    (a & 1) == (b & 1) && ((a + 2) % 24 == b || (b + 2) % 24 == a)
}

impl Library {
    /// The rows related to `track` under `criterion`, in collection order.
    /// A track past the end of the library relates to nothing.
    #[must_use]
    pub fn related_rows(&self, track: Row, criterion: RelatedCriterion) -> Vec<Row> {
        let at = track as usize;
        if at >= self.len() {
            return Vec::new();
        }
        let rows = 0..u32::try_from(self.len()).unwrap_or(u32::MAX);
        match criterion {
            RelatedCriterion::BpmAndKey => {
                let bpm = u64::from(self.bpm_x100.get(at).copied().unwrap_or(0));
                let key = self.key.get(at).map_or(u32::MAX, |&id| camelot_rank(self.keys.name(id)));
                // A track with no BPM or no key is matched on what it has;
                // with neither there is nothing to relate it by.
                if bpm == 0 && key == u32::MAX {
                    return Vec::new();
                }
                let spread = bpm * BPM_TOLERANCE_X100 / 10_000;
                rows.filter(|&r| {
                    if r == track {
                        return false;
                    }
                    let other = usize::try_from(r).unwrap_or(usize::MAX);
                    if bpm != 0 {
                        let theirs = u64::from(self.bpm_x100.get(other).copied().unwrap_or(0));
                        if theirs.abs_diff(bpm) > spread {
                            return false;
                        }
                    }
                    if key != u32::MAX {
                        let theirs = self.key.get(other).map_or(u32::MAX, |&id| camelot_rank(self.keys.name(id)));
                        if !keys_go_together(key, theirs) {
                            return false;
                        }
                    }
                    true
                })
                .collect()
            }
            RelatedCriterion::SameGenreRecent => {
                let genre = self.genre.get(at).copied().unwrap_or(0);
                if genre == 0 {
                    return Vec::new();
                }
                let since = today().minus(RECENT_DAYS, "days").days();
                rows.filter(|&r| {
                    if r == track {
                        return false;
                    }
                    let other = usize::try_from(r).unwrap_or(usize::MAX);
                    self.genre.get(other).copied() == Some(genre)
                        && Date::parse(self.date_added.get(other)).is_some_and(|added| added.days() >= since)
                })
                .collect()
            }
            RelatedCriterion::SameArtist => {
                let artist = self.artist.get(at).copied().unwrap_or(0);
                if artist == 0 {
                    return Vec::new();
                }
                rows.filter(|&r| r != track && self.artist.get(usize::try_from(r).unwrap_or(usize::MAX)).copied() == Some(artist))
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_goes_with_itself_its_relative_and_its_neighbours() {
        let a1 = camelot_rank("Abm");
        let b1 = camelot_rank("B");
        let a2 = camelot_rank("Ebm");
        let a12 = camelot_rank("Dbm");
        let a3 = camelot_rank("Bbm");
        assert!(keys_go_together(a1, a1));
        assert!(keys_go_together(a1, b1));
        assert!(keys_go_together(a1, a2));
        assert!(keys_go_together(a1, a12));
        assert!(!keys_go_together(a1, a3));
        assert!(!keys_go_together(a1, camelot_rank("F#")));
        assert!(!keys_go_together(u32::MAX, a1));
    }
}
