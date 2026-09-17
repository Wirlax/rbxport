//! Musical key by chromagram plus profile matching.
//!
//! Spectral energy is folded onto the twelve pitch classes, then correlated
//! against major and minor profiles. The best of the 24 rotations is the key.
//!
//! The front end is what decides the result, and two things about it were
//! measured to matter more than any profile:
//!
//! - **Energy, not counts.** An earlier version added `ln(1 + |X|)` for every
//!   bin near a semitone, which made a pitch class's total mostly the number
//!   of bins that happen to round to it — a fixed pattern that read most of
//!   the golden playlist as C minor. A bin contributes its magnitude, and
//!   only in proportion to how close it sits to a semitone.
//! - **Harmonics fold down.** A note at f also puts energy at 2f, 3f, 4f…,
//!   which land on other pitch classes (the fifth, the major third) and pull
//!   the answer a fifth away. Each bin also credits the classes of f/2, f/3,
//!   f/4 with decaying weight, so a note's harmonics vote for the note.
//!
//! Every choice is a field of [`KeyOptions`] so the golden rig can search
//! them; the defaults are what scored best on the golden playlist.

use realfft::RealFftPlanner;

/// Chromagram window.
///
/// This must be much longer than the onset window: at 44.1 kHz a 1024-sample
/// FFT resolves 43 Hz, while a semitone at 220 Hz spans about 13 Hz — too
/// coarse to tell pitch classes apart at all in the bass. 8192 gives ~5.4 Hz,
/// enough down to about 90 Hz.
const KEY_FRAME: usize = 8192;
/// Frames advance by half a window.
const KEY_HOP: usize = 4096;

/// Names matching `djmdKey.ScaleName`, so a detected key can be interned
/// against rekordbox's own table.
const MAJOR_NAMES: [&str; 12] =
    ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MINOR_NAMES: [&str; 12] =
    ["Cm", "Dbm", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "Abm", "Am", "Bbm", "Bm"];

/// A pair of key profiles: how much each scale degree belongs to a major
/// key and to a minor one, from the tonic up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub major: [f64; 12],
    pub minor: [f64; 12],
}

impl Profile {
    /// Krumhansl & Kessler's probe-tone ratings.
    pub const KRUMHANSL: Self = Self {
        major: [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88],
        minor: [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17],
    };
    /// Temperley's revision of them.
    pub const TEMPERLEY: Self = Self {
        major: [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0],
        minor: [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0],
    };
    /// Shaath's, tuned on popular music.
    pub const SHAATH: Self = Self {
        major: [6.6, 2.0, 3.5, 2.3, 4.6, 4.0, 2.5, 5.2, 2.4, 3.7, 2.3, 3.4],
        minor: [6.5, 2.7, 3.5, 5.4, 2.6, 3.5, 2.5, 5.2, 4.0, 2.7, 4.3, 3.2],
    };
    /// Faraldo's, fitted to electronic dance music.
    pub const EDMA: Self = Self {
        major: [
            0.165_195_51, 0.047_490_26, 0.084_738_18, 0.060_529_32, 0.092_413_94, 0.104_866_93,
            0.053_340_33, 0.124_411_28, 0.061_519_13, 0.077_734_83, 0.048_344_89, 0.079_415_44,
        ],
        minor: [
            0.172_353_48, 0.053_364_89, 0.076_285_6, 0.100_341_43, 0.056_336_06, 0.088_297_44,
            0.050_648_11, 0.117_429_37, 0.076_787_74, 0.056_318_83, 0.058_750_83, 0.053_086_24,
        ],
    };
    /// The notes of the key and nothing else: tonic and fifth carry it, the
    /// rest of the scale supports.
    pub const DIATONIC: Self = Self {
        major: [3.0, 0.0, 1.0, 0.0, 2.0, 1.0, 0.0, 2.5, 0.0, 1.0, 0.0, 1.0],
        minor: [3.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 2.5, 1.0, 0.0, 1.0, 0.5],
    };
}

/// Everything the front end and the matcher are tuned by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyOptions {
    pub profile: Profile,
    /// Lowest and highest frequency a bin may have to count.
    pub low_hz: f64,
    pub high_hz: f64,
    /// Exponent applied to a bin's magnitude: 1 sums magnitudes, 2 energy,
    /// and a fraction compresses loud partials.
    pub power: f64,
    /// How many subharmonics a bin credits (1 is just its own pitch class),
    /// and how much each further one counts relative to the one before.
    pub harmonics: usize,
    pub harmonic_decay: f64,
    /// Whether each frame's chroma is scaled to a peak of 1 before it is
    /// added, so a loud drop does not outweigh a quiet intro.
    pub frame_norm: bool,
    /// Added to a minor key's score. Dance music is mostly minor, and a
    /// profile correlation alone has no way to know that.
    pub minor_bias: f64,
    /// Weight of a second chroma taken from the bass alone (`low_hz` up to
    /// `bass_high_hz`), added to the full-range one. The bass line states
    /// the key of a dance track more plainly than anything above it.
    pub bass_weight: f64,
    pub bass_high_hz: f64,
    /// Whether to find the track's tuning first. A track a third of a
    /// semitone off concert pitch puts every note between two classes.
    pub tuning: bool,
}

/// Sub-bins per semitone in a frame's chroma: enough to see a track sitting
/// between semitones.
pub const SUBBINS: usize = 3;
/// Sub-bins per octave.
pub const BINS: usize = 12 * SUBBINS;

/// The defaults are the best of 3,600 combinations measured on the golden
/// playlist (`golden key`): 130 of 155 exact, 138 within a fifth or the
/// relative key. Of the 25 misses, 11 are the same tonic in the other mode
/// — mostly tracks rekordbox calls major — 8 are a fifth away and 6 are
/// elsewhere. Each of these was tried and did not move the count: a mode
/// decision from the third above the tonic, a separate smaller bias for
/// the mode, the tuning estimate, a bass-only chroma added in, per-frame
/// normalisation, and profiles learned from the playlist itself with each
/// track held out (132 at best, within noise of the shipped 130).
impl Default for KeyOptions {
    fn default() -> Self {
        Self {
            profile: Profile::EDMA,
            low_hz: 55.0,
            // Above 2 kHz there is little but hats and the upper partials of
            // everything; 1 kHz, 1.5 kHz, 3 kHz and 5 kHz all scored lower.
            high_hz: 2000.0,
            power: 1.0,
            harmonics: 4,
            harmonic_decay: 0.6,
            frame_norm: false,
            // 89 % of the library is minor. Anything from 0.3 to 0.5 scores
            // the same; below 0.3 the relative major wins too often.
            minor_bias: 0.3,
            bass_weight: 0.0,
            bass_high_hz: 250.0,
            // Measured to change nothing on the golden playlist, which is
            // mastered at concert pitch throughout; kept for material that
            // is not.
            tuning: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicalKey {
    /// e.g. `Ebm`.
    pub name: String,
    /// 0 = C.
    pub tonic: u8,
    pub minor: bool,
}

impl MusicalKey {
    /// Camelot code, e.g. `2A`.
    pub fn camelot(&self) -> String {
        // The wheel is the circle of fifths: 1A is Abm and 1B is B, so each
        // family is anchored on its own tonic (Ab = 8, B = 11).
        let anchor = if self.minor { 8 } else { 11 };
        let index = (i32::from(self.tonic) - anchor).rem_euclid(12);
        let number = (index * 7).rem_euclid(12) + 1;
        format!("{number}{}", if self.minor { 'A' } else { 'B' })
    }
}

/// Detects the key. Returns `None` when the audio is too short or has no
/// discernible pitch content, rather than guessing C major.
pub fn detect_key(samples: &[f32], sample_rate: u32) -> Option<MusicalKey> {
    detect_key_with(samples, sample_rate, KeyOptions::default())
}

/// Detects the key with the front end and matcher under the caller's control.
#[allow(clippy::needless_pass_by_value, reason = "a Copy options struct")]
pub fn detect_key_with(samples: &[f32], sample_rate: u32, options: KeyOptions) -> Option<MusicalKey> {
    let frames = chroma_frames(samples, sample_rate, options)?;
    let offset = if options.tuning { tuning_offset(&frames) } else { 0 };
    let mut chroma = fold_frames(&frames, options.frame_norm, offset);
    if options.bass_weight > 0.0 {
        let bass = KeyOptions { high_hz: options.bass_high_hz, harmonics: 1, ..options };
        if let Some(bass_frames) = chroma_frames(samples, sample_rate, bass) {
            let bass_chroma = fold_frames(&bass_frames, options.frame_norm, offset);
            let scale = options.bass_weight * total(&chroma) / total(&bass_chroma).max(f64::EPSILON);
            for (slot, b) in chroma.iter_mut().zip(bass_chroma.iter()) {
                *slot += b * scale;
            }
        }
    }
    best_key(&chroma, options)
}

/// The key that best explains a chroma.
pub fn best_key(chroma: &[f64; 12], options: KeyOptions) -> Option<MusicalKey> {
    if total(chroma) <= f64::EPSILON {
        return None;
    }
    let mut best: Option<(f64, usize, bool)> = None;
    for tonic in 0..12 {
        for minor in [false, true] {
            let profile = if minor { &options.profile.minor } else { &options.profile.major };
            let score = correlate(chroma, profile, tonic) + if minor { options.minor_bias } else { 0.0 };
            if best.is_none_or(|(b, _, _)| score > b) {
                best = Some((score, tonic, minor));
            }
        }
    }
    let (_, tonic, minor) = best?;
    let names = if minor { MINOR_NAMES } else { MAJOR_NAMES };
    Some(MusicalKey { name: (*names.get(tonic)?).to_owned(), tonic: u8::try_from(tonic).ok()?, minor })
}

fn total(chroma: &[f64; 12]) -> f64 {
    chroma.iter().sum()
}

/// Which sub-bin the track's notes centre on: the tuning, as a signed
/// number of sub-bins from concert pitch.
///
/// Whichever of the three sub-bins per semitone collects the most energy
/// across the whole track is where the notes are.
pub fn tuning_offset(frames: &[[f64; BINS]]) -> i32 {
    let mut by_offset = [0.0_f64; SUBBINS];
    for frame in frames {
        for (i, value) in frame.iter().enumerate() {
            by_offset[i % SUBBINS] += value;
        }
    }
    let best = by_offset
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(0, |(i, _)| i);
    // Sub-bin 0 is the semitone's centre, 1 a third sharp, 2 a third flat.
    if best == SUBBINS - 1 { -1 } else { best as i32 }
}

/// How much a class's neighbouring sub-bins count: a third of a semitone
/// off the centre is the skirt of a note more often than a note, so it
/// counts a quarter.
const NEIGHBOUR_WEIGHT: f64 = 0.25;

/// Sums per-frame chroma into twelve classes, each frame scaled to a peak
/// of 1 first when asked. `offset` is the tuning from [`tuning_offset`]:
/// the sub-bin at the centre of each class, with its two neighbours at
/// [`NEIGHBOUR_WEIGHT`].
pub fn fold_frames(frames: &[[f64; BINS]], frame_norm: bool, offset: i32) -> [f64; 12] {
    let mut out = [0.0_f64; 12];
    for frame in frames {
        let mut folded = [0.0_f64; 12];
        for (class, slot) in folded.iter_mut().enumerate() {
            let centre = (class * SUBBINS) as i32 + offset;
            let at = |i: i32| frame[(i.rem_euclid(BINS as i32)) as usize];
            *slot = at(centre) + NEIGHBOUR_WEIGHT * (at(centre - 1) + at(centre + 1));
        }
        let peak = folded.iter().fold(0.0_f64, |a, &b| a.max(b));
        if peak <= 0.0 {
            continue;
        }
        let scale = if frame_norm { 1.0 / peak } else { 1.0 };
        for (slot, value) in out.iter_mut().zip(folded.iter()) {
            *slot += value * scale;
        }
    }
    out
}

/// Pearson correlation between the chroma and a profile rotated to `tonic`.
fn correlate(chroma: &[f64; 12], profile: &[f64; 12], tonic: usize) -> f64 {
    let rotated: Vec<f64> = (0..12)
        .map(|i| profile.get((i + 12 - tonic) % 12).copied().unwrap_or(0.0))
        .collect();
    let mean_c: f64 = chroma.iter().sum::<f64>() / 12.0;
    let mean_p: f64 = rotated.iter().sum::<f64>() / 12.0;
    let mut num = 0.0;
    let mut den_c = 0.0;
    let mut den_p = 0.0;
    for i in 0..12 {
        let dc = chroma.get(i).copied().unwrap_or(0.0) - mean_c;
        let dp = rotated.get(i).copied().unwrap_or(0.0) - mean_p;
        num += dc * dp;
        den_c += dc * dc;
        den_p += dp * dp;
    }
    if den_c <= 0.0 || den_p <= 0.0 {
        return 0.0;
    }
    num / (den_c.sqrt() * den_p.sqrt())
}

/// One chroma per frame at `SUBBINS` per semitone, un-normalised. `None`
/// when the audio is too short.
pub fn chroma_frames(samples: &[f32], sample_rate: u32, options: KeyOptions) -> Option<Vec<[f64; BINS]>> {
    if samples.len() < KEY_FRAME * 2 || sample_rate == 0 {
        return None;
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(KEY_FRAME);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    let window: Vec<f32> = (0..KEY_FRAME)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::PI * 2.0 * i as f32 / KEY_FRAME as f32).cos())
        .collect();

    // What each bin contributes, worked out once: the sub-bins it credits
    // (its own pitch and its subharmonics') and the weight of each. A bin
    // between two sub-bins is split between them in proportion, so nothing
    // is dropped for falling between the cracks: down at 110 Hz a sub-bin
    // is under half an FFT bin wide.
    let hz_per_bin = f64::from(sample_rate) / KEY_FRAME as f64;
    let mut credits: Vec<Vec<(usize, f64)>> = Vec::with_capacity(output.len());
    for bin in 0..output.len() {
        let freq = bin as f64 * hz_per_bin;
        let mut list = Vec::new();
        if bin > 0 && freq >= options.low_hz && freq <= options.high_hz {
            let mut weight = 1.0;
            for h in 1..=options.harmonics.max(1) {
                let midi = 69.0 + 12.0 * (freq / h as f64 / 440.0).log2();
                let sub = midi * SUBBINS as f64;
                let below = sub.floor();
                let frac = sub - below;
                let index = |x: f64| (x as i64).rem_euclid(BINS as i64) as usize;
                list.push((index(below), weight * (1.0 - frac)));
                list.push((index(below + 1.0), weight * frac));
                weight *= options.harmonic_decay;
            }
        }
        credits.push(list);
    }

    let mut frames = Vec::with_capacity(samples.len() / KEY_HOP);
    let mut start = 0;
    while start + KEY_FRAME <= samples.len() {
        let Some(chunk) = samples.get(start..start + KEY_FRAME) else { break };
        for (i, slot) in input.iter_mut().enumerate() {
            *slot = chunk.get(i).copied().unwrap_or(0.0) * window.get(i).copied().unwrap_or(0.0);
        }
        if fft.process(&mut input, &mut output).is_err() {
            break;
        }
        let mut chroma = [0.0_f64; BINS];
        for (bin, value) in output.iter().enumerate() {
            let Some(list) = credits.get(bin) else { continue };
            if list.is_empty() {
                continue;
            }
            let magnitude = f64::from(value.norm()).powf(options.power);
            for &(index, weight) in list {
                chroma[index] += magnitude * weight;
            }
        }
        frames.push(chroma);
        start += KEY_HOP;
    }
    Some(frames)
}
