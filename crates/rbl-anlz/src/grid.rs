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
//!
//! An edit applies to the whole grid or, through [`apply_from`], to the
//! beats from one point on. That is how a track with a tempo change is
//! gridded: the beats before the change are left as they were and the ones
//! after are re-spaced at the new tempo, each beat carrying its own tempo
//! as rekordbox's own multi-tempo grids do.

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
    /// Re-space the beats at this tempo, holding a beat at `anchor_ms`: the
    /// beat nearest the anchor moves onto it and keeps its number, and the
    /// rest are laid out from there over the span the grid covered.
    Tempo { bpm_x100: u16, anchor_ms: u32 },
    /// The tempo changed by this many hundredths of a BPM, the first beat
    /// held where it is: rekordbox's widen and narrow buttons.
    Stretch { by_x100: i32 },
    /// Move the grid so the beat nearest this time lands exactly on it.
    Align { time_ms: u32 },
}

/// The most beats a re-spaced grid may hold: a three-hour mix at the
/// highest tempo the tag can store is under half of this.
const MAX_BEATS: usize = 1 << 20;

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
        Edit::Tempo { bpm_x100, anchor_ms } => retempo(beats, bpm_x100, anchor_ms),
        Edit::Stretch { by_x100 } => {
            let tempo = i64::from(tempo_x100(beats)).saturating_add(i64::from(by_x100));
            let bpm_x100 = u16::try_from(tempo.clamp(1, i64::from(u16::MAX))).unwrap_or(1);
            retempo(beats, bpm_x100, beats.first().map_or(0, |beat| beat.time_ms))
        }
        Edit::Align { time_ms } => {
            let at = beats.get(nearest(beats, time_ms)).map_or(0, |beat| beat.time_ms);
            let by = i64::from(time_ms) - i64::from(at);
            nudge(beats, i32::try_from(by).unwrap_or(if by < 0 { i32::MIN } else { i32::MAX }))
        }
    }
}

/// Applies an edit from one point on, leaving the beats before it as they
/// were.
///
/// `from_ms` names the first beat to change: the one nearest that time.
/// `None` is the whole grid. A changed beat that would land on or before
/// the last untouched one is dropped rather than put out of order — a grid
/// the player reads must be ascending.
#[must_use]
pub fn apply_from(beats: &[Beat], from_ms: Option<u32>, edit: Edit) -> Vec<Beat> {
    let Some(from_ms) = from_ms else { return apply(beats, edit) };
    if beats.is_empty() {
        return Vec::new();
    }
    let at = nearest(beats, from_ms);
    let (head, tail) = beats.split_at(at);
    let last_kept = head.last().map(|beat| beat.time_ms);
    let mut out = head.to_vec();
    out.extend(
        apply(tail, edit).into_iter().filter(|beat| last_kept.is_none_or(|kept| beat.time_ms > kept)),
    );
    out
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

/// Index of the beat closest to `time_ms`, or `None` for an empty grid.
#[must_use]
pub fn nearest_index(beats: &[Beat], time_ms: u32) -> Option<usize> {
    (!beats.is_empty()).then(|| nearest(beats, time_ms))
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
                time_ms: beat.time_ms + next.time_ms.saturating_sub(beat.time_ms) / 2,
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

/// Lays the grid out again at `bpm_x100`, a beat held at `anchor_ms`.
///
/// The beat nearest the anchor moves onto it and keeps its number; the rest
/// are placed a beat apart either side of it, each from the anchor rather
/// than from its neighbour so nothing accumulates, over the span the old
/// grid covered — half a beat past its first and last beats, so the count
/// stays about what it was. A tempo of zero cannot be laid out and leaves
/// the grid alone.
fn retempo(beats: &[Beat], bpm_x100: u16, anchor_ms: u32) -> Vec<Beat> {
    let (Some(first), Some(last)) = (beats.first(), beats.last()) else { return Vec::new() };
    if bpm_x100 == 0 {
        return beats.to_vec();
    }
    let beat_ms = 6_000_000.0 / f64::from(bpm_x100);
    let anchor = f64::from(anchor_ms);
    let number = beats.get(nearest(beats, anchor_ms)).map_or(1, |beat| beat.beat_number);
    // The span the old grid covered, widened to take in an anchor set
    // outside it — a tap before the first beat is still a beat.
    let lo = f64::from(first.time_ms).min(anchor) - beat_ms / 2.0;
    let hi = f64::from(last.time_ms).max(anchor) + beat_ms / 2.0;
    // Whole beats either side of the anchor, one past the span each way so
    // the strict test below is what decides; clamped through f64 so an
    // absurd span cannot overflow.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "clamped to a count that fits before the cast; the cap is 2^20, exact in f64"
    )]
    let bound = |edge: f64| ((edge - anchor) / beat_ms).clamp(-(MAX_BEATS as f64), MAX_BEATS as f64) as i64;
    let k_min = bound(lo) - 1;
    let k_max = bound(hi) + 1;
    let mut out = Vec::with_capacity(usize::try_from(k_max - k_min + 1).unwrap_or(0).min(MAX_BEATS));
    for k in k_min..=k_max {
        #[allow(clippy::cast_precision_loss, reason = "a beat count, far below 2^52")]
        let time = anchor + k as f64 * beat_ms;
        // Strictly inside: a beat exactly half a beat past the old first or
        // last beat would be a beat the old grid did not reach.
        if time < 1.0 || time <= lo || time >= hi {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "positive and under u32::MAX by the span")]
        let time_ms = time.round().min(f64::from(u32::MAX)) as u32;
        out.push(Beat {
            beat_number: bar_position(i64::from(number) - 1 + k),
            tempo_x100: bpm_x100,
            time_ms,
        });
        if out.len() >= MAX_BEATS {
            break;
        }
    }
    out
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
        for edit in [
            Edit::Nudge(10),
            Edit::Double,
            Edit::Halve,
            Edit::Downbeat { time_ms: 0 },
            Edit::Tempo { bpm_x100: 12_000, anchor_ms: 0 },
            Edit::Stretch { by_x100: 1 },
            Edit::Align { time_ms: 100 },
        ] {
            assert!(apply(&[], edit).is_empty());
            assert!(apply_from(&[], Some(100), edit).is_empty());
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

    #[test]
    fn a_new_tempo_holds_the_anchor_beat_and_lays_the_rest_out_from_it() {
        // 100 BPM is a beat every 600 ms. Anchored on the beat at 2000 ms,
        // which is the fourth (a 4), and the grid still spans about 500..4000.
        let slowed = apply(&grid(), Edit::Tempo { bpm_x100: 10_000, anchor_ms: 2000 });
        assert!(slowed.iter().all(|b| b.tempo_x100 == 10_000));
        let anchor = slowed.iter().find(|b| b.time_ms == 2000).expect("the anchor beat is kept");
        assert_eq!(anchor.beat_number, 4, "the anchor keeps its number");
        assert_eq!(times(&slowed), vec![800, 1400, 2000, 2600, 3200, 3800]);
        assert_eq!(numbers(&slowed), vec![2, 3, 4, 1, 2, 3], "the count runs on either side of the anchor");
    }

    #[test]
    fn a_new_tempo_moves_the_nearest_beat_onto_an_anchor_between_beats() {
        // Tapped at 2100 ms: the beat at 2000 comes to 2100 and keeps its 4.
        let tapped = apply(&grid(), Edit::Tempo { bpm_x100: 12_000, anchor_ms: 2100 });
        assert!(times(&tapped).contains(&2100));
        assert_eq!(tapped.iter().find(|b| b.time_ms == 2100).unwrap().beat_number, 4);
        assert_eq!(times(&tapped), vec![600, 1100, 1600, 2100, 2600, 3100, 3600, 4100]);
    }

    #[test]
    fn a_zero_tempo_cannot_be_laid_out_and_changes_nothing() {
        assert_eq!(apply(&grid(), Edit::Tempo { bpm_x100: 0, anchor_ms: 500 }), grid());
    }

    #[test]
    fn a_new_tempo_never_places_a_beat_at_or_before_the_start() {
        let early = apply(&grid(), Edit::Tempo { bpm_x100: 12_000, anchor_ms: 200 });
        assert!(early.iter().all(|b| b.time_ms > 0));
        assert_eq!(times(&early)[..2], [200, 700]);
    }

    #[test]
    fn stretching_changes_the_tempo_by_hundredths_with_the_first_beat_held() {
        let narrower = apply(&grid(), Edit::Stretch { by_x100: 1 });
        assert_eq!(tempo_x100(&narrower), 12_001);
        assert_eq!(narrower[0].time_ms, 500, "the first beat does not move");
        assert_eq!(narrower[0].beat_number, 1);
        // 0.01 BPM over seven beats is a hair under half a millisecond.
        assert_eq!(times(&narrower), vec![500, 1000, 1500, 2000, 2500, 3000, 3500, 4000]);
        let wider = apply(&grid(), Edit::Stretch { by_x100: -1000 });
        assert_eq!(tempo_x100(&wider), 11_000);
        // 110 BPM is 545.45 ms a beat.
        assert_eq!(times(&wider)[..3], [500, 1045, 1591]);
    }

    #[test]
    fn stretching_cannot_leave_the_tempo_the_tag_can_store() {
        assert_eq!(tempo_x100(&apply(&grid(), Edit::Stretch { by_x100: -20_000 })), 1);
        assert_eq!(tempo_x100(&apply(&grid(), Edit::Stretch { by_x100: 100_000 })), u16::MAX);
    }

    #[test]
    fn aligning_moves_the_grid_so_the_nearest_beat_lands_on_the_time() {
        // 1530 is nearest the beat at 1500, so the whole grid moves 30 ms later.
        let aligned = apply(&grid(), Edit::Align { time_ms: 1530 });
        assert_eq!(times(&aligned), vec![530, 1030, 1530, 2030, 2530, 3030, 3530, 4030]);
        assert_eq!(numbers(&aligned), numbers(&grid()));
        // And earlier, when the time is before the nearest beat.
        let back = apply(&grid(), Edit::Align { time_ms: 1470 });
        assert_eq!(times(&back)[..3], [470, 970, 1470]);
    }

    #[test]
    fn an_edit_from_a_point_leaves_the_beats_before_it_alone() {
        // From the beat at 2500 (the fifth) on, at 100 BPM.
        let split = apply_from(&grid(), Some(2480), Edit::Tempo { bpm_x100: 10_000, anchor_ms: 2500 });
        assert_eq!(times(&split)[..4], [500, 1000, 1500, 2000], "the head is untouched");
        assert!(split[..4].iter().all(|b| b.tempo_x100 == 12_000));
        assert_eq!(times(&split)[4..], [2500, 3100, 3700]);
        assert!(split[4..].iter().all(|b| b.tempo_x100 == 10_000));
        // The anchor was a 1 and still is; the count runs on.
        assert_eq!(numbers(&split), vec![1, 2, 3, 4, 1, 2, 3]);
    }

    #[test]
    fn an_edit_from_a_point_drops_beats_that_would_cross_the_kept_ones() {
        // Nudging the tail 700 ms earlier would put its first beat (2500 →
        // 1800) before the last kept beat at 2000: that beat goes.
        let split = apply_from(&grid(), Some(2500), Edit::Nudge(-700));
        assert_eq!(times(&split), vec![500, 1000, 1500, 2000, 2300, 2800, 3300]);
    }

    #[test]
    fn halving_from_a_point_keeps_the_head_and_the_tails_downbeat() {
        let split = apply_from(&grid(), Some(2500), Edit::Halve);
        assert_eq!(times(&split), vec![500, 1000, 1500, 2000, 2500, 3500]);
        assert_eq!(numbers(&split), vec![1, 2, 3, 4, 1, 2]);
        assert_eq!(split[4].tempo_x100, 6_000);
        assert_eq!(split[0].tempo_x100, 12_000);
    }

    #[test]
    fn an_edit_from_nowhere_in_particular_is_the_whole_grid() {
        assert_eq!(apply_from(&grid(), None, Edit::Nudge(10)), apply(&grid(), Edit::Nudge(10)));
    }

    #[test]
    fn the_nearest_beat_is_found_or_absent() {
        assert_eq!(nearest_index(&grid(), 1600), Some(2));
        assert_eq!(nearest_index(&[], 1600), None);
    }
}
