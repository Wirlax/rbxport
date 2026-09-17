//! Tempo and beat grid.
//!
//! Three stages, each reading the onset envelope rather than the audio:
//!
//! 1. **Candidates.** The autocorrelation of the envelope peaks at the beat
//!    period and at every multiple of it; the Fourier transform of the
//!    envelope peaks at the beat rate and every multiple of *that*. A wrong
//!    period at three halves of the beat correlates (it lands on kick, hat,
//!    kick, hat) but has no spectral line, so scoring a candidate by both
//!    leaves the octave as the only ambiguity, and a tempo prior settles it.
//! 2. **Fit.** The chosen period is refined to a fraction of an envelope
//!    sample by a comb, the phase is found the same way, and then every beat
//!    is snapped to the nearest onset peak and a line is fitted through the
//!    snapped beats. Six hundred beats average a five-millisecond hop down to
//!    a fraction of a millisecond, which is what a grid needs to stay on the
//!    beat for a whole track: 0.05 BPM of error is a beat and a half of drift
//!    by the end of a five-minute track.
//! 3. **Segments.** A tempo is measured in windows along the track, and a
//!    stretch that sustains a different one becomes its own segment with its
//!    own fit, the boundary placed at the beat where the onsets change sides.
//!    DJ edits jump tempo mid-track; a single line through such a track is
//!    wrong on both sides of the jump.
//!
//! The downbeat is not chosen here: every beat comes back numbered from the
//! first, and [`crate::downbeat`] renumbers the grid once it has looked at
//! the music.

use crate::attack::AttackMap;
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

/// A stretch of the track at one tempo: every `phase_secs + k × period_secs`
/// that falls in `from_secs..to_secs` is a beat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Where the stretch begins, in seconds from the start of the file: 0
    /// for the first segment, the tempo change for the others.
    pub from_secs: f64,
    /// Where it ends: the next change, or the end of the track.
    pub to_secs: f64,
    /// Seconds between beats.
    pub period_secs: f64,
    /// Any beat's time; the grid is this plus whole periods either way.
    pub phase_secs: f64,
}

impl Segment {
    pub fn bpm(&self) -> f64 {
        if self.period_secs > 0.0 { 60.0 / self.period_secs } else { 0.0 }
    }
    /// The first beat, in seconds from the start of the file.
    pub fn start_secs(&self) -> f64 {
        if self.period_secs <= 0.0 {
            return self.from_secs;
        }
        let k = ((self.from_secs - self.phase_secs) / self.period_secs).ceil();
        self.phase_secs + k * self.period_secs
    }
    /// Beats in the segment.
    pub fn beats(&self) -> usize {
        if self.period_secs <= 0.0 || self.to_secs <= self.from_secs {
            return 0;
        }
        let first = self.start_secs();
        if first >= self.to_secs {
            return 0;
        }
        // A whole number of periods, give or take rounding, is that many
        // beats, not one more.
        ((self.to_secs - first) / self.period_secs - 1e-9).ceil() as usize
    }
    /// The same grid moved half a beat, which is where the beats are when
    /// the grid was built on the off-beat.
    #[must_use]
    pub fn shifted_half_beat(&self) -> Self {
        Self { phase_secs: self.phase_secs + self.period_secs / 2.0, ..*self }
    }
}

/// Every beat of every segment, in order, numbered so that beat `i` is a
/// downbeat when `(i + phase) % 4 == 0`.
pub fn beats_of(segments: &[Segment], phase: usize) -> Vec<Beat> {
    let mut beats = Vec::new();
    for segment in segments {
        let tempo_x100 = u16::try_from((segment.bpm() * 100.0).round() as i64).unwrap_or(0);
        let start = segment.start_secs();
        for i in 0..segment.beats().min(100_000) {
            let time = start + i as f64 * segment.period_secs;
            beats.push(Beat {
                beat_number: u16::try_from((beats.len() + phase) % 4 + 1).unwrap_or(1),
                tempo_x100,
                time_ms: u32::try_from((time * 1000.0).round() as i64).unwrap_or(0),
            });
        }
    }
    beats
}

#[derive(Debug, Clone)]
pub struct TempoResult {
    /// The tempo at the start of the track, which is what a library shows.
    pub bpm: f64,
    /// Confidence in 0..=1: how far the chosen candidate stands out from the
    /// next best that is not a multiple of it.
    pub confidence: f64,
    /// Seconds from the start to the first beat.
    pub first_beat_secs: f64,
    /// One entry per tempo, in order. A track at one tempo has one.
    pub segments: Vec<Segment>,
    /// Every beat, numbered 1..=4 cyclically from the first.
    pub beats: Vec<Beat>,
}

impl TempoResult {
    pub fn empty() -> Self {
        Self { bpm: 0.0, confidence: 0.0, first_beat_secs: 0.0, segments: Vec::new(), beats: Vec::new() }
    }
}

/// What the candidate stage is tuned by.
///
/// Held in a struct so a measurement rig can search them against real audio.
/// The defaults are what ships; anything else has to beat them on the golden
/// playlist before it becomes a default.
#[derive(Debug, Clone, Copy)]
pub struct TempoOptions {
    pub min_bpm: f64,
    pub max_bpm: f64,
    /// Where the tempo prior is centred, in BPM.
    pub prior_centre: f64,
    /// The prior's spread, in natural logs of tempo ratio.
    pub prior_width: f64,
    /// Phases tried when the comb refines a period.
    pub refine_phases: usize,
    /// Seconds per window when looking for a tempo change.
    pub segment_window_secs: f64,
    /// A window whose own tempo differs from the track's by more than this
    /// fraction, and which is followed by another that agrees with it, starts
    /// a new segment.
    pub segment_threshold: f64,
    /// Where each beat is placed when the line is fitted.
    pub placement: Placement,
}

/// What a beat is snapped to before the line is fitted through the beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The nearest peak of the onset envelope, sharpened by a parabola:
    /// within a hop of the hit.
    Envelope,
    /// The start of the kick's click in the 900–9000 Hz band, to the
    /// millisecond ([`crate::attack`]). Falls back to the envelope where no
    /// attack map is given.
    Attack,
}

impl Default for TempoOptions {
    fn default() -> Self {
        Self {
            min_bpm: MIN_BPM,
            max_bpm: MAX_BPM,
            prior_centre: 132.0,
            prior_width: 0.5,
            refine_phases: 32,
            segment_window_secs: 16.0,
            segment_threshold: 0.02,
            placement: Placement::Attack,
        }
    }
}

/// One tempo the candidate stage considered, with everything it was scored
/// on. Exposed so a measurement rig can print the table for a track that
/// chose wrongly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub bpm: f64,
    /// Normalised autocorrelation at the beat period.
    pub acf: f64,
    /// Fourier magnitude at the beat rate, relative to the strongest in range.
    pub fourier: f64,
    /// The tempo prior at this BPM.
    pub prior: f64,
    /// What it was ranked by.
    pub score: f64,
}

/// Estimates tempo and builds the beat grid from the envelope alone.
pub fn detect_tempo(onsets: &OnsetEnvelope) -> TempoResult {
    detect_tempo_with(onsets, None, TempoOptions::default())
}

/// Where the fit reads its beats from: the envelope, and the attack map
/// when there is one.
#[derive(Clone, Copy)]
struct Reader<'a> {
    values: &'a [f64],
    rate: f64,
    origin_secs: f64,
    attacks: Option<&'a AttackMap>,
}

impl Reader<'_> {
    /// The beat nearest `x` (an envelope sample), as an envelope sample and
    /// a weight, from the attack map if there is one and it finds a spike,
    /// else from the envelope.
    fn snap(&self, x: f64, reach: f64) -> Option<(f64, f64)> {
        if let Some(attacks) = self.attacks {
            let secs = self.origin_secs + x / self.rate;
            // The attack search is capped at the map's own reach: the
            // prediction is already within a few milliseconds of the hit,
            // and a wider window catches the clap after the kick, which
            // the line then chases pass after pass.
            let reach_secs = (reach / self.rate).min(attacks.options().reach_secs);
            if let Some(attack) = attacks.attack_within(secs, reach_secs) {
                return Some(((attack.secs - self.origin_secs) * self.rate, attack.height));
            }
            return None;
        }
        local_peak(self.values, x, reach)
    }
}

/// Estimates tempo with the candidate stage under the caller's control,
/// placing each beat on the kick's attack when an attack map is given.
#[allow(clippy::needless_pass_by_value, reason = "a Copy options struct")]
pub fn detect_tempo_with(onsets: &OnsetEnvelope, attacks: Option<&AttackMap>, options: TempoOptions) -> TempoResult {
    if onsets.len() < 64 || onsets.rate <= 0.0 {
        return TempoResult::empty();
    }
    let values: Vec<f64> = onsets.values.iter().map(|&v| f64::from(v)).collect();
    let attacks = match options.placement {
        Placement::Attack => attacks,
        Placement::Envelope => None,
    };
    let reader = Reader { values: &values, rate: onsets.rate, origin_secs: onsets.origin_secs, attacks };

    let candidates = candidates(&values, onsets.rate, options);
    let Some(best) = candidates.first() else {
        return TempoResult::empty();
    };
    // Confidence: the winner against the best candidate that is not a
    // multiple of it, both of them prior-free.
    let rival = candidates
        .iter()
        .skip(1)
        .find(|c| !related(c.bpm, best.bpm))
        .map_or(0.0, |c| c.acf * c.fourier);
    let own = best.acf * best.fourier;
    let confidence = if own > 0.0 { ((own - rival) / own).clamp(0.0, 1.0) } else { 0.0 };

    // The whole track at the winning tempo, then split where it changes.
    let segments = segment(reader, best.bpm, options);
    let beats = beats_of(&segments, 0);
    let (bpm, first_beat_secs) = segments.first().map_or((0.0, 0.0), |s| (s.bpm(), s.start_secs()));
    TempoResult { bpm, confidence, first_beat_secs, segments, beats }
}

/// The candidate stage alone, best first. For measurement.
pub fn tempo_candidates(onsets: &OnsetEnvelope, options: TempoOptions) -> Vec<Candidate> {
    if onsets.len() < 64 || onsets.rate <= 0.0 {
        return Vec::new();
    }
    let values: Vec<f64> = onsets.values.iter().map(|&v| f64::from(v)).collect();
    candidates(&values, onsets.rate, options)
}

/// Whether two tempos are a simple ratio of each other.
fn related(a: f64, b: f64) -> bool {
    if a <= 0.0 || b <= 0.0 {
        return false;
    }
    let ratio = if a > b { a / b } else { b / a };
    [1.0, 2.0, 3.0, 4.0, 1.5, 4.0 / 3.0]
        .iter()
        .any(|r| (ratio - r).abs() < 0.02 * r)
}

// ---------------------------------------------------------------- candidates

/// Every tempo worth considering, scored, best first.
fn candidates(values: &[f64], rate: f64, options: TempoOptions) -> Vec<Candidate> {
    let n = values.len();
    let mean = values.iter().sum::<f64>() / n as f64;
    let centred: Vec<f64> = values.iter().map(|v| v - mean).collect();
    let variance = centred.iter().map(|v| v * v).sum::<f64>() / n as f64;
    if variance <= 0.0 {
        return Vec::new();
    }

    let lag_of = |bpm: f64| rate * 60.0 / bpm;
    // Lags out to twice the slowest tempo, so a candidate's own double can be
    // read for whichever candidate asks.
    let min_lag = (lag_of(options.max_bpm * 2.0).floor() as usize).max(2);
    let max_lag = (lag_of(options.min_bpm / 2.0).ceil() as usize).min(n / 2);
    if max_lag <= min_lag + 2 {
        return Vec::new();
    }
    let acf = autocorrelation(&centred, variance, min_lag, max_lag);
    let acf_at = |lag: f64| interpolate(&acf, lag - min_lag as f64);

    // Peaks of the autocorrelation inside the range, with their multiples
    // and simple fractions, so that the octave a listener would choose is
    // in the running even when it is not the strongest correlation.
    let in_range = |bpm: f64| bpm >= options.min_bpm && bpm <= options.max_bpm;
    let mut bpms: Vec<f64> = Vec::new();
    let lo = lag_of(options.max_bpm).floor() as usize;
    let hi = lag_of(options.min_bpm).ceil() as usize;
    for lag in lo.max(min_lag + 1)..hi.min(max_lag - 1) {
        let here = acf_at(lag as f64);
        if here <= acf_at(lag as f64 - 1.0) || here < acf_at(lag as f64 + 1.0) || here <= 0.0 {
            continue;
        }
        // Parabolic interpolation of the peak.
        let a = acf_at(lag as f64 - 1.0);
        let c = acf_at(lag as f64 + 1.0);
        let denom = a - 2.0 * here + c;
        let offset = if denom.abs() > f64::EPSILON { 0.5 * (a - c) / denom } else { 0.0 };
        let bpm = rate * 60.0 / (lag as f64 + offset.clamp(-0.5, 0.5));
        for ratio in [1.0, 2.0, 0.5, 1.5, 2.0 / 3.0, 3.0, 1.0 / 3.0, 4.0 / 3.0, 0.75] {
            let relative = bpm * ratio;
            if in_range(relative) && !bpms.iter().any(|b| (b - relative).abs() < relative * 0.01) {
                bpms.push(relative);
            }
        }
    }
    if bpms.is_empty() {
        return Vec::new();
    }

    // The Fourier magnitude at every candidate rate, and the strongest in
    // range to normalise by.
    let fourier_raw: Vec<f64> = bpms.iter().map(|&bpm| fourier_magnitude(&centred, rate, bpm)).collect();
    let fourier_peak = fourier_raw.iter().fold(0.0_f64, |a, &b| a.max(b));
    let acf_peak = bpms.iter().map(|&bpm| acf_at(lag_of(bpm))).fold(0.0_f64, f64::max);

    let mut out: Vec<Candidate> = bpms
        .iter()
        .zip(&fourier_raw)
        .map(|(&bpm, &ft)| {
            let acf = (acf_at(lag_of(bpm)) / acf_peak.max(f64::EPSILON)).max(0.0);
            let fourier = if fourier_peak > 0.0 { ft / fourier_peak } else { 0.0 };
            let prior = tempo_prior(bpm, options);
            // The Fourier term enters as a square root: its job is to rule
            // out a period with no spectral line at all (a hundredth of the
            // strongest), not to prefer the hat rate over the beat, where
            // it is louder by half.
            Candidate { bpm, acf, fourier, prior, score: acf * fourier.sqrt() * prior }
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Normalised autocorrelation for lags `min_lag..=max_lag`.
fn autocorrelation(centred: &[f64], variance: f64, min_lag: usize, max_lag: usize) -> Vec<f64> {
    let n = centred.len();
    (min_lag..=max_lag)
        .map(|lag| {
            let count = n.saturating_sub(lag);
            if count == 0 {
                return 0.0;
            }
            let sum: f64 = centred.iter().zip(centred.iter().skip(lag)).map(|(a, b)| a * b).sum();
            sum / (count as f64 * variance)
        })
        .collect()
}

/// Linear interpolation into a table, clamped at the ends.
fn interpolate(table: &[f64], x: f64) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    let x = x.clamp(0.0, (table.len() - 1) as f64);
    let i = x.floor() as usize;
    let frac = x - i as f64;
    let a = table.get(i).copied().unwrap_or(0.0);
    let b = table.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * frac
}

/// Seconds per window of the Fourier magnitude.
///
/// Not the whole track: a sum over five minutes resolves 0.2 BPM, so a
/// candidate read off the autocorrelation a tenth of a BPM from the truth
/// drifts through most of a cycle and cancels itself. Twenty-second windows
/// resolve 3 BPM, which is coarse enough to be robust to that and fine
/// enough to keep a tempo apart from its three-halves relative.
const FOURIER_WINDOW_SECS: f64 = 20.0;

/// Magnitude of the envelope's Fourier component at one tempo, averaged
/// over Hann windows of `FOURIER_WINDOW_SECS`.
fn fourier_magnitude(centred: &[f64], rate: f64, bpm: f64) -> f64 {
    let omega = 2.0 * std::f64::consts::PI * bpm / 60.0 / rate;
    let window = ((FOURIER_WINDOW_SECS * rate) as usize).clamp(64, centred.len().max(64));
    let hop = window / 2;
    // A rotating phasor rather than a sin and cos per sample: the envelope
    // is fifty thousand samples long and there are dozens of candidates.
    let (step_re, step_im) = (omega.cos(), omega.sin());
    let mut total = 0.0;
    let mut windows = 0.0;
    let mut start = 0;
    while start + window <= centred.len() {
        let (mut re, mut im) = (1.0_f64, 0.0_f64);
        let (mut sum_re, mut sum_im) = (0.0_f64, 0.0_f64);
        for (i, &v) in centred[start..start + window].iter().enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / window as f64).cos();
            let w = v * hann;
            sum_re += w * re;
            sum_im += w * im;
            let next_re = re * step_re - im * step_im;
            im = re * step_im + im * step_re;
            re = next_re;
        }
        total += (sum_re * sum_re + sum_im * sum_im).sqrt() / window as f64;
        windows += 1.0;
        start += hop;
        if start + window > centred.len() && windows == 0.0 {
            break;
        }
    }
    if windows > 0.0 { total / windows } else { 0.0 }
}

/// How readily a tempo is heard *as* the tempo: a log-normal centred where
/// dance music sits. It only breaks ties between octaves.
fn tempo_prior(bpm: f64, options: TempoOptions) -> f64 {
    if bpm <= 0.0 || options.prior_width <= 0.0 || options.prior_centre <= 0.0 {
        return 0.0;
    }
    let x = (bpm / options.prior_centre).ln() / options.prior_width;
    (-0.5 * x * x).exp()
}

// ---------------------------------------------------------------- fitting

/// A grid fitted to one stretch of the envelope: period and phase in
/// envelope samples.
#[derive(Debug, Clone, Copy)]
struct Fit {
    /// Envelope sample of the first beat, which may be negative: the grid is
    /// extended back to the start of the file.
    phase: f64,
    period: f64,
}

/// Fits a constant-tempo grid to `from..to` of the envelope near `bpm`.
fn fit(reader: Reader<'_>, bpm: f64, from: usize, to: usize, options: TempoOptions) -> Option<Fit> {
    let slice = reader.values.get(from..to)?;
    if slice.len() < 8 {
        return None;
    }
    let coarse = reader.rate * 60.0 / bpm;
    let period = refine_period(slice, coarse, options.refine_phases);
    let phase = best_phase(slice, period, 64);
    let mut fit = Fit { phase: phase + from as f64, period };
    // Snap and refit, three times: each pass moves the line a little closer
    // to the onsets and the next pass then snaps a few more of them.
    for _ in 0..3 {
        let Some(next) = snap_and_fit(reader, from, to, fit) else { break };
        fit = next;
    }
    Some(fit)
}

/// Comb-filter score of a grid: onset energy summed at every beat of the
/// period from `phase`, normalised per beat.
fn comb(values: &[f64], period: f64, phase: f64) -> f64 {
    if period < 2.0 || values.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut count = 0.0;
    let mut x = phase;
    while x < values.len() as f64 {
        sum += sample_at(values, x);
        count += 1.0;
        x += period;
    }
    if count > 0.0 { sum / count } else { 0.0 }
}

/// Linear interpolation between envelope samples.
fn sample_at(values: &[f64], x: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let i = x.floor() as usize;
    let frac = x - i as f64;
    let a = values.get(i).copied().unwrap_or(0.0);
    let b = values.get(i + 1).copied().unwrap_or(0.0);
    a + (b - a) * frac
}

/// Finds the fractional period near `coarse` that best explains the onsets.
///
/// Integer lags quantise the tempo badly: at 172 envelope samples per second
/// the lags either side of 128 BPM are 1.6 BPM apart. A fine search over
/// fractional periods, each scored at its best phase, removes that.
fn refine_period(values: &[f64], coarse: f64, phases: usize) -> f64 {
    let score = |period: f64| {
        let steps = phases.max(1);
        (0..steps)
            .map(|step| comb(values, period, period * step as f64 / steps as f64))
            .fold(0.0_f64, f64::max)
    };
    let mut best = (score(coarse), coarse);
    // Two passes: a coarse sweep of ±1 sample, then a fine one around the
    // winner. The sweep is what costs, so it is kept to a few hundred combs.
    for (span, step) in [(1.0, 0.02), (0.03, 0.002)] {
        let centre = best.1;
        let mut candidate = centre - span;
        while candidate <= centre + span {
            let s = score(candidate);
            if s > best.0 {
                best = (s, candidate);
            }
            candidate += step;
        }
    }
    best.1
}

/// The phase in `0..period` at which the comb collects the most energy.
fn best_phase(values: &[f64], period: f64, phases: usize) -> f64 {
    let steps = phases.max(1);
    let mut best = (f64::NEG_INFINITY, 0.0);
    for step in 0..steps {
        let phase = period * step as f64 / steps as f64;
        let s = comb(values, period, phase);
        if s > best.0 {
            best = (s, phase);
        }
    }
    // Sharpen with a parabola through the neighbours: the comb is smooth in
    // phase at the scale of one step.
    let step = period / steps as f64;
    let (a, b, c) = (
        comb(values, period, best.1 - step),
        best.0,
        comb(values, period, best.1 + step),
    );
    let denom = a - 2.0 * b + c;
    let offset = if denom.abs() > f64::EPSILON { 0.5 * (a - c) / denom } else { 0.0 };
    best.1 + offset.clamp(-1.0, 1.0) * step
}

/// Snaps every predicted beat in `from..to` to the nearest onset — the
/// kick's attack when there is an attack map, else the envelope's peak —
/// and fits a line through the snapped ones, weighted by the hit's
/// strength.
fn snap_and_fit(reader: Reader<'_>, from: usize, to: usize, current: Fit) -> Option<Fit> {
    let period = current.period;
    if period < 2.0 {
        return None;
    }
    // A beat may be snapped this far: less than half a beat, so that two
    // predictions cannot claim one onset, and less than a hat's distance.
    let reach = (period * 0.2).max(1.0);
    let first = ((from as f64 - current.phase) / period).ceil() as i64;
    let last = ((to as f64 - 1.0 - current.phase) / period).floor() as i64;
    if last < first + 4 {
        return None;
    }

    // Every predicted beat snapped to its nearest onset peak, with the
    // peak's height as its weight: a beat in a breakdown, with no onset to
    // snap to, should not pull the line.
    let mut snapped: Vec<(f64, f64, f64)> = Vec::new(); // (index, time, weight)
    for k in first..=last {
        let predicted = current.phase + k as f64 * period;
        if let Some((at, height)) = reader.snap(predicted, reach) {
            snapped.push((k as f64, at, height));
        }
    }
    if snapped.len() < 4 {
        return None;
    }
    // Beats with next to no onset are left out altogether rather than
    // merely down-weighted. An intro of pads has a noise-floor peak near
    // every predicted beat, and a minute of those, however light, tilts a
    // line that is then extrapolated back across the same minute.
    let mut heights: Vec<f64> = snapped.iter().map(|&(_, _, h)| h).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = heights.get(heights.len() / 2).copied().unwrap_or(0.0) * 0.2;
    let strong: Vec<(f64, f64, f64)> = snapped.iter().copied().filter(|&(_, _, h)| h >= floor).collect();
    let snapped = if strong.len() >= 4 { strong } else { snapped };

    // Weighted least squares of snapped time on beat index, twice: the
    // second pass leaves out the beats that sit furthest from the first
    // line. A section of swung or late-hitting percussion snaps its beats
    // consistently off the grid, and left in, it bends a whole track's
    // tempo by a hundredth of a BPM, which is a beat of drift by the end.
    let line = |points: &[(f64, f64, f64)]| -> Option<(f64, f64)> {
        let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(x, y, w) in points {
            sw += w;
            sx += w * x;
            sy += w * y;
            sxx += w * x * x;
            sxy += w * x * y;
        }
        let denom = sw * sxx - sx * sx;
        if sw <= 0.0 || denom.abs() < f64::EPSILON {
            return None;
        }
        let slope = (sw * sxy - sx * sy) / denom;
        Some((slope, (sy - slope * sx) / sw))
    };
    let (slope, intercept) = line(&snapped)?;
    let mut residuals: Vec<f64> = snapped.iter().map(|&(x, y, _)| (y - intercept - slope * x).abs()).collect();
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Keep the closest four fifths, and never anything further than a
    // tenth of a beat from the line.
    let cutoff = residuals
        .get(residuals.len() * 4 / 5)
        .copied()
        .unwrap_or(f64::INFINITY)
        .min(period * 0.1);
    let kept: Vec<(f64, f64, f64)> = snapped
        .iter()
        .copied()
        .filter(|&(x, y, _)| (y - intercept - slope * x).abs() <= cutoff)
        .collect();
    let (slope, intercept) = if kept.len() >= 4 { line(&kept)? } else { (slope, intercept) };

    // A fit that walked away from the candidate period is a fit to the wrong
    // onsets; keep the old one.
    if (slope - period).abs() > period * 0.02 {
        return None;
    }
    Some(Fit { phase: intercept, period: slope })
}

/// The highest envelope sample within `reach` of `around`, with its
/// position sharpened by a parabola through its neighbours.
fn local_peak(values: &[f64], around: f64, reach: f64) -> Option<(f64, f64)> {
    let lo = (around - reach).floor().max(0.0) as usize;
    let hi = ((around + reach).ceil() as usize).min(values.len().saturating_sub(1));
    if lo >= hi {
        return None;
    }
    let mut best = (0.0_f64, lo);
    for i in lo..=hi {
        let v = values.get(i).copied().unwrap_or(0.0);
        if v > best.0 {
            best = (v, i);
        }
    }
    if best.0 <= 0.0 {
        return None;
    }
    let i = best.1;
    let a = if i > 0 { values.get(i - 1).copied().unwrap_or(0.0) } else { best.0 };
    let c = values.get(i + 1).copied().unwrap_or(0.0);
    let denom = a - 2.0 * best.0 + c;
    let offset = if denom.abs() > f64::EPSILON { 0.5 * (a - c) / denom } else { 0.0 };
    Some((i as f64 + offset.clamp(-0.5, 0.5), best.0))
}

// ---------------------------------------------------------------- segments

/// The tempo of each window along the track as a ratio to `bpm`, with the
/// window's start in envelope samples. For measurement.
pub fn local_tempos(onsets: &OnsetEnvelope, bpm: f64, options: TempoOptions) -> Vec<(f64, Option<f64>)> {
    let values: Vec<f64> = onsets.values.iter().map(|&v| f64::from(v)).collect();
    let window = ((options.segment_window_secs * onsets.rate) as usize).max(64);
    let hop = window / 2;
    let mut out = Vec::new();
    let mut start = 0;
    while start + window <= values.len() {
        out.push((onsets.time_of(start as f64), local_tempo(&values[start..start + window], onsets.rate, bpm, options)));
        start += hop;
    }
    out
}

/// How the fit stage arrives at a segment's tempo, for measurement: the BPM
/// from the comb alone, then after each snap-and-refit pass.
pub fn fit_report(onsets: &OnsetEnvelope, attacks: Option<&AttackMap>, bpm: f64, options: TempoOptions) -> Vec<f64> {
    let values: Vec<f64> = onsets.values.iter().map(|&v| f64::from(v)).collect();
    let n = values.len();
    let reader = Reader { values: &values, rate: onsets.rate, origin_secs: onsets.origin_secs, attacks };
    let coarse = onsets.rate * 60.0 / bpm;
    let period = refine_period(&values, coarse, options.refine_phases);
    let phase = best_phase(&values, period, 64);
    let mut out = vec![onsets.rate * 60.0 / period];
    let mut fit = Fit { phase, period };
    for _ in 0..3 {
        let Some(next) = snap_and_fit(reader, 0, n, fit) else { break };
        fit = next;
        out.push(onsets.rate * 60.0 / fit.period);
    }
    out
}

/// The walk between two tempos, for measurement: every beat it placed
/// from `from_secs`, as `(seconds, bpm from the previous beat)`, and why it
/// stopped.
pub fn walk_report(
    onsets: &OnsetEnvelope,
    attacks: Option<&AttackMap>,
    from_bpm: f64,
    to_bpm: f64,
    from_secs: f64,
    to_secs: f64,
) -> (Vec<(f64, f64)>, &'static str) {
    let values: Vec<f64> = onsets.values.iter().map(|&v| f64::from(v)).collect();
    let reader = Reader { values: &values, rate: onsets.rate, origin_secs: onsets.origin_secs, attacks };
    let x_of = |secs: f64| (secs - onsets.origin_secs) * onsets.rate;
    let a_period = onsets.rate * 60.0 / from_bpm;
    let b_period = onsets.rate * 60.0 / to_bpm;
    // Phase the old grid from the strongest hit near `from_secs`.
    let mut t = reader.snap(x_of(from_secs), a_period * 0.5).map_or(x_of(from_secs), |(at, _)| at);
    let mut period = a_period;
    let mut out = Vec::new();
    let mut why = "reached the end";
    while out.len() < MAX_WALKED_BEATS && t < x_of(to_secs) {
        let predicted = t + period;
        let Some((next, _)) = reader.snap(predicted, (RAMP_REACH * period).max(1.0)) else {
            why = "no hit within reach";
            break;
        };
        let next_period = next - t;
        if next_period <= 0.0 || (next_period - period).abs() > period * RAMP_STEP {
            why = "jump larger than the ramp step";
            out.push((onsets.time_of(next), onsets.rate * 60.0 / next_period));
            break;
        }
        period = next_period;
        t = next;
        out.push((onsets.time_of(t), onsets.rate * 60.0 / period));
        if (period - b_period).abs() <= b_period * SETTLED_TOLERANCE {
            why = "settled";
            break;
        }
    }
    (out, why)
}

/// Windows in a row that must agree on a new tempo before it is believed.
/// Three of them, hopping half a window, cover two windows' worth of
/// track: a shorter stretch is a fill, not a section.
const SEGMENT_MIN_WINDOWS: usize = 3;

/// Whether a ratio between two tempos is one a rhythm produces on its own.
///
/// A dotted-eighth delay puts a real period at four thirds of the beat; a
/// triplet feel at three halves. Windows that measure such a period have not
/// changed tempo, and a DJ edit that happens to jump by exactly that ratio
/// is rarer than the pattern.
fn rhythmic_ratio(ratio: f64) -> bool {
    [1.5, 2.0 / 3.0, 4.0 / 3.0, 0.75].iter().any(|r| (ratio - r).abs() < 0.03 * r)
}

/// Labels every window with a tempo and cuts the track into stretches to
/// fit: `(settled from, settled to, bpm, where the change after it may
/// begin)`, in order.
///
/// The tempos the track holds are the track's own plus any ratio that at
/// least `SEGMENT_MIN_WINDOWS` windows agree on (within the threshold),
/// that differs from the track's, and that is not a ratio a rhythm
/// produces. Every window is assigned to the nearest tempo or carries the
/// previous window's when it says nothing clearly; a lone window between
/// two of the other tempo is absorbed; leading windows that say nothing
/// take the first tempo heard. A stretch's settled part is its assigned
/// windows: the carried ones between two tempos are the change.
fn label_runs(
    local: &[Option<f64>],
    starts: &[usize],
    window: usize,
    n: usize,
    bpm: f64,
    options: TempoOptions,
) -> Vec<(usize, usize, f64, usize)> {
    let differs = |ratio: f64| (ratio - 1.0).abs() > options.segment_threshold && !rhythmic_ratio(ratio);
    let mut clusters: Vec<(f64, usize)> = Vec::new(); // (mean ratio, count)
    for ratio in local.iter().flatten().copied().filter(|&r| differs(r)) {
        match clusters.iter_mut().find(|(centre, _)| (ratio - *centre).abs() < options.segment_threshold) {
            Some((centre, count)) => {
                *centre = (*centre * *count as f64 + ratio) / (*count as f64 + 1.0);
                *count += 1;
            }
            None => clusters.push((ratio, 1)),
        }
    }
    clusters.retain(|&(_, count)| count >= SEGMENT_MIN_WINDOWS);

    let assigned: Vec<Option<f64>> = local
        .iter()
        .map(|ratio| {
            ratio.and_then(|r| {
                std::iter::once(1.0)
                    .chain(clusters.iter().map(|&(c, _)| c))
                    .filter(|c| (r - c).abs() < options.segment_threshold * 1.5)
                    .min_by(|a, b| (r - a).abs().partial_cmp(&(r - b).abs()).unwrap_or(std::cmp::Ordering::Equal))
            })
        })
        .collect();
    let mut previous = assigned.iter().flatten().next().copied().unwrap_or(1.0);
    let mut labels: Vec<f64> = vec![1.0; local.len()];
    for (i, value) in assigned.iter().enumerate() {
        previous = value.unwrap_or(previous);
        labels[i] = previous;
    }
    for i in 1..labels.len().saturating_sub(1) {
        if (labels[i - 1] - labels[i + 1]).abs() < 1e-9 && (labels[i] - labels[i - 1]).abs() > 1e-9 {
            labels[i] = labels[i - 1];
        }
    }

    let mut runs: Vec<(usize, usize, f64, usize)> = Vec::new();
    let mut i = 0;
    while i < labels.len() {
        let mut end = i;
        while end < labels.len() && (labels[end] - labels[i]).abs() < 1e-9 {
            end += 1;
        }
        let is_settled = |w: usize| assigned.get(w).copied().flatten().is_some_and(|r| (r - labels[i]).abs() < 1e-9);
        let first_settled = (i..end).find(|&w| is_settled(w)).unwrap_or(i);
        let last_settled = (i..end).rev().find(|&w| is_settled(w)).unwrap_or(end - 1);
        let from = if i == 0 { 0 } else { starts.get(first_settled).copied().unwrap_or(0) };
        let to = if end >= labels.len() { n } else { (starts[last_settled] + window).min(n) };
        // A window still settled at this tempo may end past the change;
        // the change is looked for from that window's start.
        let change_from = starts.get(last_settled).copied().unwrap_or(from);
        runs.push((from, to, bpm * labels[i], change_from));
        i = end;
    }
    if runs.is_empty() {
        runs.push((0, n, bpm, n));
    }
    runs
}

/// Splits the track where the tempo changes and fits each stretch.
fn segment(reader: Reader<'_>, bpm: f64, options: TempoOptions) -> Vec<Segment> {
    let (values, rate, origin_secs) = (reader.values, reader.rate, reader.origin_secs);
    let n = values.len();
    let window = ((options.segment_window_secs * rate) as usize).max(64);
    // Local tempo per window, as a ratio to the track's, or None where the
    // window has too little to say.
    let hop = window / 2;
    let mut local: Vec<Option<f64>> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut start = 0;
    while start + window <= n {
        starts.push(start);
        local.push(local_tempo(&values[start..start + window], rate, bpm, options));
        start += hop;
    }

    let mut segments: Vec<Segment> = Vec::new();
    let runs = label_runs(&local, &starts, window, n, bpm, options);

    let mut fits: Vec<(usize, usize, Fit)> = Vec::new(); // (change from, settled to, fit)
    for (from, to, run_bpm, change_from) in runs {
        let Some(f) = fit(reader, run_bpm, from, to, options) else { continue };
        fits.push((change_from, to, f));
    }
    if fits.is_empty() {
        return segments;
    }

    // Between each pair of fits: walk the change bar by bar from the old
    // grid until the bars come out at the new tempo. Each walked bar is a
    // segment of its own, and the new grid is re-phased to start at the
    // last walked downbeat. Where the walk finds nothing — a break with no
    // kicks — the change goes where the onsets change sides.
    let to_secs = |x: f64| origin_secs + x / rate;
    let mut from_sample = -origin_secs * rate;
    let mut pending: Option<Fit> = None; // the fit whose segment is open
    // A fit over a stretch is refitted over the stretch its segment
    // actually covers, once that is known: the settled windows alone can
    // be a short stretch, and every hit up to the change sharpens the
    // line, with the snapping leaving out the hits that have drifted.
    let refit = |current: Fit, from: f64, to: f64| -> Fit {
        let bpm_here = rate * 60.0 / current.period;
        let (from_i, to_i) = (from.max(0.0) as usize, (to as usize).min(n));
        if to_i <= from_i {
            return current;
        }
        fit(reader, bpm_here, from_i, to_i, options).unwrap_or(current)
    };
    for (index, &(change_from, _, f)) in fits.iter().enumerate() {
        let current = pending.unwrap_or(f);
        let Some(&(_, to_next, next)) = fits.get(index + 1) else {
            // The last fit runs to the end of the track.
            if (n as f64) > from_sample {
                let current = if pending.is_some() { current } else { refit(current, from_sample, n as f64) };
                segments.push(Segment {
                    from_secs: to_secs(from_sample),
                    to_secs: to_secs(n as f64),
                    period_secs: current.period / rate,
                    phase_secs: to_secs(current.phase),
                });
            }
            break;
        };
        // The walk starts in the old tempo's last settled window and may
        // run on into the new tempo's settled stretch until it settles.
        let bars = walk_bars(reader, current, next, change_from, to_next);
        let (cut, next_fit) = match bars.first() {
            Some(&(first_bar_start, _)) => {
                let last_end = bars.last().map_or(first_bar_start, |&(_, end)| end);
                (first_bar_start, Fit { phase: last_end, period: next.period })
            }
            // No ramp to follow: the change is a cut, placed where the new
            // beat arrives at full strength, searched over the gap and the
            // new tempo's settled stretch.
            None => (boundary(values, change_from, to_next, current, next), next),
        };
        if cut > from_sample {
            // A grid re-phased at a walked downbeat keeps that phase; any
            // other is refitted over its whole stretch.
            let current = if pending.is_some() { current } else { refit(current, from_sample, cut) };
            segments.push(Segment {
                from_secs: to_secs(from_sample),
                to_secs: to_secs(cut),
                period_secs: current.period / rate,
                phase_secs: to_secs(current.phase),
            });
        }
        for &(start, end) in &bars {
            segments.push(Segment {
                from_secs: to_secs(start),
                to_secs: to_secs(end),
                period_secs: (end - start) / 4.0 / rate,
                phase_secs: to_secs(start),
            });
        }
        from_sample = bars.last().map_or(cut, |&(_, end)| end);
        pending = Some(next_fit);
    }
    segments
}

/// The tempo of one window as a ratio to `bpm`, or None when the window has
/// no clear beat. Octaves of the track's tempo count as agreeing: a
/// breakdown that keeps only the hats is not a tempo change.
fn local_tempo(window: &[f64], rate: f64, bpm: f64, options: TempoOptions) -> Option<f64> {
    let n = window.len();
    let mean = window.iter().sum::<f64>() / n as f64;
    let centred: Vec<f64> = window.iter().map(|v| v - mean).collect();
    let variance = centred.iter().map(|v| v * v).sum::<f64>() / n as f64;
    if variance <= 0.0 {
        return None;
    }
    let lag_of = |b: f64| rate * 60.0 / b;
    let min_lag = (lag_of(options.max_bpm).floor() as usize).max(2);
    let max_lag = (lag_of(options.min_bpm).ceil() as usize).min(n / 3);
    if max_lag <= min_lag + 2 {
        return None;
    }
    let acf = autocorrelation(&centred, variance, min_lag, max_lag);
    let home = interpolate(&acf, lag_of(bpm) - min_lag as f64);
    // The strongest peak in the window.
    let mut best = (0.0_f64, 0usize);
    for i in 1..acf.len() - 1 {
        if acf[i] > acf[i - 1] && acf[i] >= acf[i + 1] && acf[i] > best.0 {
            best = (acf[i], i);
        }
    }
    if best.0 <= 0.0 {
        return None;
    }
    let peak_bpm = rate * 60.0 / (best.1 + min_lag) as f64;
    // Fold onto the octave of the track's tempo.
    let mut ratio = peak_bpm / bpm;
    while ratio > 1.5 {
        ratio /= 2.0;
    }
    while ratio < 0.75 {
        ratio *= 2.0;
    }
    // The track's own tempo still correlates well: no change. A different
    // period has to win outright, by a margin, to be believed.
    if home >= best.0 * 0.6 {
        return Some(1.0);
    }
    Some(ratio)
}

/// A walked bar counts as settled at the new tempo when its period is
/// within this fraction of the new fit's.
const SETTLED_TOLERANCE: f64 = 0.01;
/// From one beat to the next the period may move by this fraction and
/// still be a change of pace; a bigger jump is a cut, not a ramp.
const RAMP_STEP: f64 = 0.05;
/// How far either side of the predicted beat the next hit is looked for,
/// as a fraction of a beat.
const RAMP_REACH: f64 = 0.1;
/// Beats the walk gives up after, so a track whose tempo never settles
/// still ends.
const MAX_WALKED_BEATS: usize = 1024;

/// Walks a tempo change beat by beat, from the last beat of `a` before
/// `from`, until a bar comes out at `b`'s tempo or the walk reaches `to`.
///
/// Each next beat is predicted from the period of the beat before it and
/// looked for within a tenth of a beat either side, through the reader
/// (the kick's attack when there is an attack map). The period may drift
/// by up to `RAMP_STEP` per beat: that follows a rise or fall, gradual or
/// not, and stops at a jump, which is a cut. Returns the walked bars as
/// `(start, end)` envelope samples — four beats each, from the first
/// walked beat — or nothing when the walk did not reach `b`'s tempo, when
/// the two tempos are too far apart to be a change of pace, or when no
/// beat could be placed.
fn walk_bars(reader: Reader<'_>, a: Fit, b: Fit, from: usize, to: usize) -> Vec<(f64, f64)> {
    if a.period < 2.0 || b.period < 2.0 {
        return Vec::new();
    }
    let ratio = b.period / a.period;
    if !(0.6..=1.67).contains(&ratio) {
        return Vec::new();
    }
    // The first beat of `a` at or after `from`, moved onto the hit nearest
    // it: the walk measures every period from a hit to a hit.
    let k = ((from as f64 - a.phase) / a.period).ceil();
    let grid_point = a.phase + k * a.period;
    let Some((mut t, _)) = reader.snap(grid_point, (RAMP_REACH * a.period).max(1.0)) else {
        return Vec::new();
    };
    let mut period = a.period;
    let mut beats = vec![t];
    let mut settled_run = 0usize;
    while beats.len() < MAX_WALKED_BEATS && t < to as f64 {
        let predicted = t + period;
        let Some((next, _)) = reader.snap(predicted, (RAMP_REACH * period).max(1.0)) else { break };
        let next_period = next - t;
        if next_period <= 0.0 || (next_period - period).abs() > period * RAMP_STEP {
            break;
        }
        period = next_period;
        t = next;
        beats.push(t);
        // Settled once a whole bar has come out at the new tempo.
        if (period - b.period).abs() <= b.period * SETTLED_TOLERANCE {
            settled_run += 1;
            if settled_run >= 4 {
                break;
            }
        } else {
            settled_run = 0;
        }
    }
    if settled_run < 4 {
        return Vec::new();
    }
    // Whole bars only; a trailing partial bar belongs to the new grid.
    beats
        .windows(5)
        .step_by(4)
        .map(|w| (w[0], w[4]))
        .collect()
}

/// How strong the new grid's beats must be, relative to the median beat of
/// its own stretch, before the change is placed: three quarters, for four
/// beats in a row. Measured on the DJ edit in the golden playlist: the
/// incoming track's beat sits at a third to a half of its eventual level
/// for three bars under the outgoing one, and rekordbox switches at the bar
/// where it reaches full level.
const BOUNDARY_STRENGTH: f64 = 0.75;
const BOUNDARY_BEATS: usize = 4;

/// The envelope sample where the grid changes from `a` to `b`.
///
/// Not where `b` first appears: in a DJ edit the next track's beat comes in
/// under the last one's breakdown, quietly, bars before it drops, and
/// rekordbox holds the old grid until the drop. So the change is placed at
/// the first beat of `b`, searched from `from` (the start of `b`'s stretch)
/// to `to` (its end), that begins `BOUNDARY_BEATS` beats in a row each at
/// least `BOUNDARY_STRENGTH` of the median beat of `b`'s whole stretch.
/// Should no beat qualify, the change goes where the onsets stop following
/// `a` and start following `b`, which is what a plain crossfade looks like.
fn boundary(values: &[f64], from: usize, to: usize, a: Fit, b: Fit) -> f64 {
    let lo = from.min(to);
    let hi = from.max(to).min(values.len());
    if hi <= lo || b.period < 2.0 {
        return lo as f64;
    }
    // Beats of each grid inside the zone, with their onset support. The
    // reach is half the fit's: a beat is supported only by an onset that is
    // nearly on it, because two grids at nearby tempos drift through each
    // other and a wide reach lets the new one claim the old one's onsets
    // for several beats every cycle.
    let beats_of = |f: Fit| -> Vec<(f64, f64)> {
        let reach = (f.period * 0.1).max(1.0);
        let first = ((lo as f64 - f.phase) / f.period).ceil() as i64;
        let last = ((hi as f64 - f.phase) / f.period).floor() as i64;
        (first..=last)
            .map(|k| {
                let at = f.phase + k as f64 * f.period;
                (at, local_peak(values, at, reach).map_or(0.0, |(_, height)| height))
            })
            .collect()
    };
    let beats_a = beats_of(a);
    let beats_b = beats_of(b);

    let mut heights: Vec<f64> = beats_b.iter().map(|(_, h)| *h).collect();
    heights.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let median = heights.get(heights.len() / 2).copied().unwrap_or(0.0);
    let threshold = median * BOUNDARY_STRENGTH;
    // Mean support of a grid's beats from `t` over the next two bars.
    let ahead = |beats: &[(f64, f64)], t: f64| -> f64 {
        let run: Vec<f64> = beats.iter().filter(|(at, _)| *at >= t).take(BOUNDARY_BEATS * 2).map(|(_, h)| *h).collect();
        if run.is_empty() { 0.0 } else { run.iter().sum::<f64>() / run.len() as f64 }
    };
    if threshold > 0.0 {
        for (i, &(cut, _)) in beats_b.iter().enumerate() {
            let run = beats_b.get(i..i + BOUNDARY_BEATS).unwrap_or(&[]);
            // Full strength, and stronger than the old grid carried on: two
            // grids at nearby tempos drift through each other, and for a
            // few beats every cycle the new one lands on the old one's
            // onsets and looks supported.
            if run.len() == BOUNDARY_BEATS
                && run.iter().all(|(_, h)| *h >= threshold)
                && ahead(&beats_b, cut) >= ahead(&beats_a, cut)
            {
                return cut;
            }
        }
    }

    // For every possible boundary (each beat of b), the support for a before
    // it plus the support for b from it on; ties go to the later cut.
    let mut best = (f64::NEG_INFINITY, hi as f64);
    for &(cut, _) in &beats_b {
        let score: f64 = beats_a.iter().filter(|(t, _)| *t < cut).map(|(_, s)| s).sum::<f64>()
            + beats_b.iter().filter(|(t, _)| *t >= cut).map(|(_, s)| s).sum::<f64>();
        if score >= best.0 {
            best = (score, cut);
        }
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
