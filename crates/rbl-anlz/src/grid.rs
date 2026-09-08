//! Editing a beat grid.
//!
//! Every edit is a pure function from one beat list to another, so the four
//! things that go wrong — losing the downbeat, drifting the phase, leaving a
//! beat before the start of the file, renumbering off by one — are testable
//! without a file, a database or a player.
//!
//! Beat numbers run 1..=4 and 1 is the downbeat, so every edit renumbers from
//! whichever beat was a downbeat before it. A grid that came back numbered
//! from the wrong beat would put the CDJ's bar counter out by up to three
//! beats, which is worse than an unedited grid.

use crate::Beat;

/// One change to a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// Move every beat by this many milliseconds. Positive is later.
    Nudge(i32),
    /// Twice the tempo: a beat added between every pair.
    Double,
    /// Half the tempo: every other beat dropped.
    Halve,
    /// Make the beat nearest this time the downbeat.
    Downbeat { time_ms: u32 },
}

/// Applies an edit, returning the new grid.
///
/// An empty grid stays empty: there is nothing to nudge and no phase to keep.
#[must_use]
pub fn apply(beats: &[Beat], edit: Edit) -> Vec<Beat> {
    if beats.is_empty() {
        return Vec::new();
    }
    match edit {
        Edit::Nudge(by) => nudge(beats, by),
        Edit::Double => double(beats),
        Edit::Halve => halve(beats),
        Edit::Downbeat { time_ms } => renumber_from(beats, nearest(beats, time_ms)),
    }
}

/// The tempo the grid describes, from its first beat.
#[must_use]
pub fn tempo_x100(beats: &[Beat]) -> u16 {
    beats.first().map_or(0, |beat| beat.tempo_x100)
}

/// Index of the first beat marked as a downbeat, or 0 when none is.
fn first_downbeat(beats: &[Beat]) -> usize {
    beats.iter().position(|beat| beat.beat_number == 1).unwrap_or(0)
}

/// Index of the beat closest to `time_ms`.
fn nearest(beats: &[Beat], time_ms: u32) -> usize {
    beats
        .iter()
        .enumerate()
        .min_by_key(|(_, beat)| beat.time_ms.abs_diff(time_ms))
        .map_or(0, |(i, _)| i)
}

/// Numbers the grid 1..=4 with `downbeat` as a 1.
fn renumber_from(beats: &[Beat], downbeat: usize) -> Vec<Beat> {
    beats
        .iter()
        .enumerate()
        .map(|(i, beat)| Beat {
            // Signed, because beats before the chosen downbeat count backwards.
            beat_number: bar_position(index(i) - index(downbeat)),
            ..*beat
        })
        .collect()
}

/// A list index as a signed number, for arithmetic that can go negative.
fn index(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Where a beat `offset` from a downbeat sits in its bar, 1..=4.
fn bar_position(offset: i64) -> u16 {
    let wrapped = offset.rem_euclid(4);
    u16::try_from(wrapped + 1).unwrap_or(1)
}

fn nudge(beats: &[Beat], by: i32) -> Vec<Beat> {
    let downbeat = first_downbeat(beats);
    let moved: Vec<Beat> = beats
        .iter()
        .map(|beat| Beat {
            time_ms: i64::from(beat.time_ms)
                .saturating_add(i64::from(by))
                .try_into()
                .unwrap_or(0),
            ..*beat
        })
        // A beat at or before the start of the file is not a beat the player
        // can seek to, so a nudge that pushes the grid off the front drops
        // those rather than piling them all onto zero.
        .filter(|beat| beat.time_ms > 0)
        .collect();
    if moved.is_empty() {
        return Vec::new();
    }
    // Dropping beats off the front must not move the bar: a beat that was a
    // downbeat still is one. Downbeats sit every fourth beat from `downbeat`,
    // so after dropping `dropped` of them the first surviving one is at
    // `downbeat - dropped` counted around the bar.
    let dropped = beats.len() - moved.len();
    let shifted = index(downbeat).saturating_sub(index(dropped)).rem_euclid(4);
    renumber_from(&moved, usize::try_from(shifted).unwrap_or(0))
}

fn double(beats: &[Beat]) -> Vec<Beat> {
    let downbeat = first_downbeat(beats);
    let mut out = Vec::with_capacity(beats.len() * 2);
    for (i, beat) in beats.iter().enumerate() {
        let tempo = beat.tempo_x100.saturating_mul(2);
        out.push(Beat { tempo_x100: tempo, ..*beat });
        // The beat between this one and the next; the last beat has no gap
        // after it to halve, so the grid gains one fewer beat than it has.
        if let Some(next) = beats.get(i + 1) {
            out.push(Beat {
                beat_number: 1,
                tempo_x100: tempo,
                time_ms: beat.time_ms + (next.time_ms - beat.time_ms) / 2,
            });
        }
    }
    // The original downbeat is at twice its index once a beat sits between
    // every pair, and it stays the downbeat.
    renumber_from(&out, downbeat * 2)
}

fn halve(beats: &[Beat]) -> Vec<Beat> {
    let downbeat = first_downbeat(beats);
    // Kept from the downbeat rather than from the start, so the beat that was
    // a 1 still is one.
    let phase = downbeat % 2;
    let out: Vec<Beat> = beats
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == phase)
        .map(|(_, beat)| Beat { tempo_x100: beat.tempo_x100 / 2, ..*beat })
        .collect();
    if out.is_empty() {
        return Vec::new();
    }
    renumber_from(&out, downbeat / 2)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Eight beats at 120 BPM starting at 500 ms, numbered from the first.
    fn grid() -> Vec<Beat> {
        (0..8)
            .map(|i| Beat {
                beat_number: (i % 4) + 1,
                tempo_x100: 12_000,
                time_ms: 500 + u32::from(i) * 500,
            })
            .collect()
    }

    fn numbers(beats: &[Beat]) -> Vec<u16> {
        beats.iter().map(|b| b.beat_number).collect()
    }

    fn times(beats: &[Beat]) -> Vec<u32> {
        beats.iter().map(|b| b.time_ms).collect()
    }

    #[test]
    fn an_empty_grid_survives_every_edit() {
        for edit in [Edit::Nudge(10), Edit::Double, Edit::Halve, Edit::Downbeat { time_ms: 0 }] {
            assert!(apply(&[], edit).is_empty());
        }
    }

    #[test]
    fn a_nudge_moves_every_beat_by_the_same_amount() {
        let moved = apply(&grid(), Edit::Nudge(-40));
        assert_eq!(times(&moved), vec![460, 960, 1460, 1960, 2460, 2960, 3460, 3960]);
        assert_eq!(numbers(&moved), numbers(&grid()), "the bar does not move with the phase");
    }

    #[test]
    fn a_nudge_off_the_front_of_the_file_drops_those_beats() {
        // A beat at or before zero is not somewhere a player can sit.
        let moved = apply(&grid(), Edit::Nudge(-1200));
        assert_eq!(times(&moved), vec![300, 800, 1300, 1800, 2300, 2800]);
        // The grid used to start on a 1; two beats earlier, it starts on a 3.
        assert_eq!(numbers(&moved), vec![3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn doubling_puts_a_beat_between_every_pair_and_keeps_the_downbeat() {
        let doubled = apply(&grid(), Edit::Double);
        assert_eq!(doubled.len(), 15, "one fewer than twice: the last beat has no gap after it");
        assert_eq!(times(&doubled)[..5], [500, 750, 1000, 1250, 1500]);
        assert!(doubled.iter().all(|b| b.tempo_x100 == 24_000));
        assert_eq!(doubled[0].beat_number, 1, "the first beat was the downbeat and still is");
        assert_eq!(numbers(&doubled)[..5], [1, 2, 3, 4, 1]);
    }

    #[test]
    fn halving_drops_every_other_beat_and_keeps_the_downbeat() {
        let halved = apply(&grid(), Edit::Halve);
        assert_eq!(times(&halved), vec![500, 1500, 2500, 3500]);
        assert!(halved.iter().all(|b| b.tempo_x100 == 6_000));
        assert_eq!(numbers(&halved), vec![1, 2, 3, 4]);
    }

    #[test]
    fn halving_a_grid_whose_downbeat_is_not_first_keeps_that_beat() {
        // Numbered 3,4,1,2,3,4,1,2 — the downbeat is the third beat.
        let mut beats = grid();
        for (i, beat) in beats.iter_mut().enumerate() {
            beat.beat_number = ((u16::try_from(i).unwrap_or(0) + 2) % 4) + 1;
        }
        let halved = apply(&beats, Edit::Halve);
        // The downbeat sits at an even index, so even beats are what survive —
        // and the beat at 1500 ms, which was the 1, still is one.
        assert_eq!(times(&halved), vec![500, 1500, 2500, 3500]);
        assert_eq!(numbers(&halved), vec![4, 1, 2, 3]);
    }

    #[test]
    fn setting_the_downbeat_renumbers_around_the_nearest_beat() {
        // 1600 is nearest the beat at 1500, which is the third.
        let fixed = apply(&grid(), Edit::Downbeat { time_ms: 1600 });
        assert_eq!(numbers(&fixed), vec![3, 4, 1, 2, 3, 4, 1, 2]);
        assert_eq!(times(&fixed), times(&grid()), "renumbering must not move a beat");
    }

    #[test]
    fn setting_the_downbeat_past_the_end_takes_the_last_beat() {
        let fixed = apply(&grid(), Edit::Downbeat { time_ms: 999_999 });
        assert_eq!(*numbers(&fixed).last().unwrap(), 1);
    }

    #[test]
    fn doubling_then_halving_returns_the_grid_it_started_from() {
        let there_and_back = apply(&apply(&grid(), Edit::Double), Edit::Halve);
        assert_eq!(times(&there_and_back), times(&grid()));
        assert_eq!(numbers(&there_and_back), numbers(&grid()));
        assert_eq!(tempo_x100(&there_and_back), tempo_x100(&grid()));
    }

    #[test]
    fn a_nudge_and_its_opposite_cancel() {
        let back = apply(&apply(&grid(), Edit::Nudge(37)), Edit::Nudge(-37));
        assert_eq!(times(&back), times(&grid()));
    }

    #[test]
    fn the_tempo_comes_from_the_first_beat() {
        assert_eq!(tempo_x100(&grid()), 12_000);
        assert_eq!(tempo_x100(&[]), 0);
    }
}
