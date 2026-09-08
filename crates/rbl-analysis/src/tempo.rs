//! Tempo and beat grid.
//!
//! Autocorrelation of the onset envelope gives the beat period; the phase is
//! then fitted by testing every offset within one beat and keeping the one that
//! lands on the most onset energy. Both steps work on the envelope rather than
//! the audio, so the cost is independent of track length in any way that matters.

use crate::onset::OnsetEnvelope;

/// Tempo search range. Dance music sits well inside this, and a wider range
/// mostly adds octave errors.
pub const MIN_BPM: f64 = 70.0;
pub const MAX_BPM: f64 = 200.0;


/// One beat of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// 1..=4, where 1 is the downbeat.
    pub beat_number: u16,
    /// BPM x100 at this beat, as ANLZ stores it.
    pub tempo_x100: u16,
    pub time_ms: u32,
}

#[derive(Debug, Clone)]
pub struct TempoResult {
    pub bpm: f64,
    /// Confidence in 0..=1: the autocorrelation peak relative to its neighbours.
    pub confidence: f64,
    /// Seconds from the start to the first beat.
    pub first_beat_secs: f64,
    pub beats: Vec<Beat>,
}

impl TempoResult {
    pub fn empty() -> Self {
        Self { bpm: 0.0, confidence: 0.0, first_beat_secs: 0.0, beats: Vec::new() }
    }
}

/// The knobs the octave choice turns on.
///
/// Held in a struct so the tuning rig can search them against real audio
/// rather than against an argument. The defaults are what ships; anything
/// else has to beat them on a test split before it becomes a default.
#[derive(Debug, Clone, Copy)]
pub struct TempoOptions {
    /// Where the tempo prior is centred, in BPM.
    pub prior_centre: f64,
    /// The prior's spread, in natural logs of tempo ratio.
    pub prior_width: f64,
    /// How many multiples of a candidate period to add into its score.
    ///
    /// 1 is plain autocorrelation. Higher values reward a period whose own
    /// multiples also correlate, which is true of the beat and not of a
    /// subdivision of it — every other multiple of a subdivision falls
    /// between beats.
    pub harmonics: usize,
    /// How much each further multiple counts, relative to the one before.
    pub harmonic_decay: f64,
    /// Phases tried when ranking one candidate period against another.
    ///
    /// Too few and a candidate is scored at a phase that does not fit it,
    /// which moves the peak off the true period by a fraction of a BPM — small
    /// enough to pass every octave check and still miss rekordbox's value.
    /// Measured on 150 tracks of the reference library, held-out half:
    ///
    /// | phases | exact within 0.05 BPM |
    /// |---|---|
    /// | 8 | 73% |
    /// | 16 | 87% |
    /// | **32** | **95%** |
    /// | 64 | 95% |
    /// | 128 | 95% |
    ///
    /// It saturates at 32, so that is what ships: 64 costs twice as much for
    /// the same answer.
    pub refine_phases: usize,
}

impl Default for TempoOptions {
    fn default() -> Self {
        Self {
            prior_centre: 126.0,
            prior_width: 0.85,
            harmonics: 1,
            harmonic_decay: 1.0,
            refine_phases: 32,
        }
    }
}

/// Estimates tempo and builds the beat grid.
pub fn detect_tempo(onsets: &OnsetEnvelope, sample_rate: u32) -> TempoResult {
    detect_tempo_with(onsets, sample_rate, TempoOptions::default())
}

/// Estimates tempo with the octave choice under the caller's control.
#[allow(clippy::needless_pass_by_value, reason = "a Copy options struct")]
pub fn detect_tempo_with(
    onsets: &OnsetEnvelope,
    _sample_rate: u32,
    options: TempoOptions,
) -> TempoResult {
    if onsets.len() < 64 || onsets.rate <= 0.0 {
        return TempoResult::empty();
    }

    // Lag range in envelope samples for the BPM range.
    let lag_for_bpm = |bpm: f64| (onsets.rate * 60.0 / bpm).round() as usize;
    let min_lag = lag_for_bpm(MAX_BPM).max(2);
    let max_lag = lag_for_bpm(MIN_BPM).min(onsets.len() / 2);
    if max_lag <= min_lag {
        return TempoResult::empty();
    }

    let values = &onsets.values;
    // Scored past the tempo range, far enough to reach every multiple the
    // harmonic term below asks for. Stopping at `max_lag` would make those
    // reads return zero — silently, and exactly the way the dead half-time
    // term did.
    let score_to = (max_lag * options.harmonics.max(1)).min(values.len().saturating_sub(1));
    let mut scores = vec![0.0_f64; score_to + 1];

    // Every lag is scored before any is chosen, so that the choice below can
    // read any lag it likes. Merging the two passes is a trap worth naming: a
    // term that compares a candidate against a *longer* lag reads zero in a
    // single ascending pass and silently does nothing, which is exactly the
    // bug that hid here.
    for lag in min_lag..=score_to {
        let mut sum = 0.0_f64;
        let mut count = 0_usize;
        for i in 0..values.len().saturating_sub(lag) {
            let a = f64::from(values.get(i).copied().unwrap_or(0.0));
            let b = f64::from(values.get(i + lag).copied().unwrap_or(0.0));
            sum += a * b;
            count += 1;
        }
        if let Some(slot) = scores.get_mut(lag) {
            *slot = if count == 0 { 0.0 } else { sum / count as f64 };
        }
    }

    let mut best = (0.0_f64, min_lag);
    for lag in min_lag..=max_lag {
        let score = scores.get(lag).copied().unwrap_or(0.0);
        // Autocorrelation cannot tell 64 from 128 from 256 BPM: every multiple
        // of the true beat correlates. A listener resolves that by preference,
        // so weight candidates by how tempo-like they are. Without this, a fifth
        // of tracks locked onto a wrong multiple even though the period itself
        // was right to a hundredth of a BPM.
        let candidate_bpm = onsets.rate * 60.0 / lag as f64;
        // Adding the candidate's own multiples separates a beat from a
        // subdivision of it: every multiple of the beat correlates, while
        // every other multiple of a subdivision lands between beats.
        let mut support = score;
        let mut weight = 1.0;
        for k in 2..=options.harmonics {
            weight *= options.harmonic_decay;
            support += weight * scores.get(lag * k).copied().unwrap_or(0.0);
        }
        let combined = support * tempo_prior(candidate_bpm, options);
        if combined > best.0 {
            best = (combined, lag);
        }
    }

    // Two corrections belong here and neither survived measurement.
    //
    // One doubled a candidate whenever the midpoint between its beats carried
    // correlation. Real tracks nearly always have hi-hats at twice the beat,
    // so it fired on tempos that were already right: 13 octave errors in 40
    // against 5 without it.
    //
    // The other re-ranked the chosen period against its metrical relatives
    // — a half, two thirds, three halves, a double — using the comb score
    // below, which unlike autocorrelation can tell a beat from three halves of
    // one. It fixed the 3:2 case it was written for and broke a 92.5 BPM case
    // in the same synthetic, and on 150 real tracks it cost three points of
    // accuracy (95% to 92%) and gained an octave error. The check is
    // symmetric: it moves a tempo in the wrong direction exactly as readily as
    // the right one. See TODO.md for both sets of numbers.
    let lag = best.1;

    if lag == 0 {
        return TempoResult::empty();
    }

    // Refine to a fractional lag.
    //
    // Integer lags quantise the tempo badly: at 172 envelope samples per second
    // the lags either side of 128 BPM are 1.6 BPM apart, which put the median
    // error at 0.375 BPM against rekordbox. Scoring fractional lags with linear
    // interpolation between envelope samples removes that entirely.
    let refined_lag = refine_lag(values, lag, options.refine_phases);
    let bpm = onsets.rate * 60.0 / refined_lag;

    // Confidence: how much the winning lag stands out from the field.
    let mean: f64 = scores.iter().skip(min_lag).sum::<f64>() / (max_lag - min_lag + 1) as f64;
    let peak = scores.get(lag).copied().unwrap_or(0.0);
    let confidence = if mean > 0.0 { ((peak / mean) - 1.0).clamp(0.0, 1.0) } else { 0.0 };

    // Phase: the offset within one beat that collects the most onset energy.
    let mut best_phase = (f64::NEG_INFINITY, 0_usize);
    for phase in 0..lag {
        let mut sum = 0.0_f64;
        let mut i = phase;
        while i < values.len() {
            sum += f64::from(values.get(i).copied().unwrap_or(0.0));
            i += lag;
        }
        if sum > best_phase.0 {
            best_phase = (sum, phase);
        }
    }

    let first_beat_secs = onsets.seconds(best_phase.1);
    let beat_secs = 60.0 / bpm;
    let total_secs = onsets.seconds(values.len());
    let beat_count = if beat_secs > 0.0 {
        ((total_secs - first_beat_secs) / beat_secs).floor().max(0.0) as usize
    } else {
        0
    };

    let tempo_x100 = u16::try_from((bpm * 100.0).round() as i64).unwrap_or(0);
    let beats = (0..beat_count.min(100_000))
        .map(|i| {
            let time = first_beat_secs + i as f64 * beat_secs;
            Beat {
                // Downbeat every four beats. Which beat is *the* downbeat needs
                // musical structure we do not detect yet, so this is a grid
                // aligned to the beat, not a claim about the bar.
                beat_number: u16::try_from(i % 4 + 1).unwrap_or(1),
                tempo_x100,
                time_ms: u32::try_from((time * 1000.0).round() as i64).unwrap_or(0),
            }
        })
        .collect();

    TempoResult { bpm, confidence, first_beat_secs, beats }
}

/// How readily a tempo is heard *as* the tempo.
///
/// A log-normal centred where dance music sits. It only breaks ties between
/// octaves — it is far too broad to move an estimate that the signal supports.
fn tempo_prior(bpm: f64, options: TempoOptions) -> f64 {
    if bpm <= 0.0 || options.prior_width <= 0.0 || options.prior_centre <= 0.0 {
        return 0.0;
    }
    let x = (bpm / options.prior_centre).ln() / options.prior_width;
    (-0.5 * x * x).exp()
}

/// Finds the fractional lag that best explains the onset envelope.
///
/// Searches a fine grid either side of the integer peak, scoring each candidate
/// by a comb filter: sum the envelope at every multiple of the candidate period,
/// reading between samples by linear interpolation. The true period maximises it.
/// How much onset energy lands on a grid of this period, at its best phase.
///
/// Normalised per beat, so a longer period is not rewarded simply for fitting
/// fewer beats into the track. This is the score that can tell a beat from a
/// subdivision of one: a grid on the real beat lands on full beats every time,
/// while one at three halves of it alternates between beats and the hi-hats
/// between them, and averages lower.
fn comb_score(values: &[f32], period: f64, phases: usize) -> f64 {
    if period < 2.0 || values.is_empty() {
        return 0.0;
    }
    let sample_at = |x: f64| -> f64 {
        if x < 0.0 {
            return 0.0;
        }
        let i = x.floor() as usize;
        let frac = x - i as f64;
        let a = f64::from(values.get(i).copied().unwrap_or(0.0));
        let b = f64::from(values.get(i + 1).copied().unwrap_or(0.0));
        a + (b - a) * frac
    };

    let mut best = 0.0_f64;
    let steps = phases.max(1);
    for step in 0..steps {
        let phase = period * step as f64 / steps as f64;
        let mut sum = 0.0;
        let mut x = phase;
        while x < values.len() as f64 {
            sum += sample_at(x);
            x += period;
        }
        let beats = ((values.len() as f64 - phase) / period).max(1.0);
        let normalised = sum / beats;
        if normalised > best {
            best = normalised;
        }
    }
    best
}

/// Finds the fractional lag that best explains the onset envelope.
///
/// Searches a fine grid either side of the integer peak. Integer lags quantise
/// the tempo badly: at 172 envelope samples per second the lags either side of
/// 128 BPM are 1.6 BPM apart, which put the median error at 0.375 BPM against
/// rekordbox. Scoring fractional lags removes that entirely.
fn refine_lag(values: &[f32], coarse: usize, phases: usize) -> f64 {
    let mut best = (comb_score(values, coarse as f64, phases), coarse as f64);
    // +/- one integer lag covers the quantisation error; 0.002 steps put the
    // residual tempo error well under 0.01 BPM.
    let mut candidate = coarse as f64 - 1.0;
    while candidate <= coarse as f64 + 1.0 {
        let score = comb_score(values, candidate, phases);
        if score > best.0 {
            best = (score, candidate);
        }
        candidate += 0.002;
    }
    best.1
}

/// Chooses the octave nearest a reference tempo.
///
/// Tempo estimation reliably finds *a* multiple of the beat; picking which one
/// a human would call the tempo needs a prior. When comparing against a known
/// value, this makes the comparison about accuracy rather than octave choice.
pub fn nearest_octave(bpm: f64, reference: f64) -> f64 {
    if bpm <= 0.0 || reference <= 0.0 {
        return bpm;
    }
    [0.25, 1.0 / 3.0, 0.5, 1.0, 2.0, 3.0, 4.0]
        .into_iter()
        .map(|m| bpm * m)
        .min_by(|a, b| {
            (a - reference)
                .abs()
                .partial_cmp(&(b - reference).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(bpm)
}
