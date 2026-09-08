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

/// Estimates tempo and builds the beat grid.
pub fn detect_tempo(onsets: &OnsetEnvelope, _sample_rate: u32) -> TempoResult {
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
    let mut scores = vec![0.0_f64; max_lag + 1];

    // Every lag is scored before any is chosen, so that the choice below can
    // read any lag it likes. Merging the two passes is a trap worth naming: a
    // term that compares a candidate against a *longer* lag reads zero in a
    // single ascending pass and silently does nothing, which is exactly the
    // bug that hid here.
    for lag in min_lag..=max_lag {
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
        let combined = score * tempo_prior(candidate_bpm);
        if combined > best.0 {
            best = (combined, lag);
        }
    }

    // A half-time correction belongs here and is not yet written. One that
    // doubled a candidate whenever the midpoint between its beats carried
    // correlation was tried and measured worse on the real library — real
    // tracks nearly always have hi-hats at twice the beat, so it fired on
    // tempos that were already right. See TODO.md for the numbers.
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
    let refined_lag = refine_lag(values, lag);
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
fn tempo_prior(bpm: f64) -> f64 {
    /// Where dance music sits.
    const CENTRE: f64 = 126.0;
    /// About one octave of spread either side.
    const WIDTH: f64 = 0.85;

    if bpm <= 0.0 {
        return 0.0;
    }
    let x = (bpm / CENTRE).ln() / WIDTH;
    (-0.5 * x * x).exp()
}

/// Finds the fractional lag that best explains the onset envelope.
///
/// Searches a fine grid either side of the integer peak, scoring each candidate
/// by a comb filter: sum the envelope at every multiple of the candidate period,
/// reading between samples by linear interpolation. The true period maximises it.
fn refine_lag(values: &[f32], coarse: usize) -> f64 {
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

    // Score a candidate period by how much energy lands on its grid, taking the
    // best phase for that period.
    let score_for = |period: f64| -> f64 {
        if period < 2.0 {
            return 0.0;
        }
        let mut best = 0.0_f64;
        // Sixteen phases is enough: we only need to rank periods against each
        // other, and the exact phase is fitted separately afterwards.
        for step in 0..16 {
            let phase = period * f64::from(step) / 16.0;
            let mut sum = 0.0;
            let mut x = phase;
            while x < values.len() as f64 {
                sum += sample_at(x);
                x += period;
            }
            // Normalise by the number of beats so longer periods are not penalised.
            let beats = ((values.len() as f64 - phase) / period).max(1.0);
            let normalised = sum / beats;
            if normalised > best {
                best = normalised;
            }
        }
        best
    };

    let mut best = (score_for(coarse as f64), coarse as f64);
    // +/- one integer lag covers the quantisation error; 0.002 steps put the
    // residual tempo error well under 0.01 BPM.
    let mut candidate = coarse as f64 - 1.0;
    while candidate <= coarse as f64 + 1.0 {
        let score = score_for(candidate);
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
