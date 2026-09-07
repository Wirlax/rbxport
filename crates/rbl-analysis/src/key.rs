//! Musical key by chromagram plus Krumhansl-Schmuckler profile matching.
//!
//! Energy is folded onto the twelve pitch classes, then correlated against
//! empirically-derived major and minor profiles. The best of the 24 rotations
//! is the key.

use realfft::RealFftPlanner;

/// Chromagram window.
///
/// This must be much longer than the onset window: at 44.1 kHz a 1024-sample
/// FFT resolves 43 Hz, while a semitone at 220 Hz spans about 13 Hz — too
/// coarse to tell pitch classes apart at all in the bass, which made an A minor
/// scale read as F major. 8192 gives ~5.4 Hz, enough down to about 90 Hz.
const KEY_FRAME: usize = 8192;

/// Krumhansl-Kessler profiles: how strongly each scale degree is perceived as
/// belonging to the key.
const MAJOR: [f64; 12] =
    [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f64; 12] =
    [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

/// Names matching `djmdKey.ScaleName`, so a detected key can be interned
/// against rekordbox's own table.
const MAJOR_NAMES: [&str; 12] =
    ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MINOR_NAMES: [&str; 12] =
    ["Cm", "Dbm", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "Abm", "Am", "Bbm", "Bm"];

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
    let chroma = chromagram(samples, sample_rate)?;
    let total: f64 = chroma.iter().sum();
    if total <= f64::EPSILON {
        return None;
    }

    let mut best: Option<(f64, usize, bool)> = None;
    for tonic in 0..12 {
        for minor in [false, true] {
            let profile = if minor { &MINOR } else { &MAJOR };
            let score = correlate(&chroma, profile, tonic);
            if best.is_none_or(|(b, _, _)| score > b) {
                best = Some((score, tonic, minor));
            }
        }
    }

    let (_, tonic, minor) = best?;
    let names = if minor { MINOR_NAMES } else { MAJOR_NAMES };
    Some(MusicalKey {
        name: (*names.get(tonic)?).to_owned(),
        tonic: u8::try_from(tonic).ok()?,
        minor,
    })
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

/// Folds spectral energy onto the twelve pitch classes.
fn chromagram(samples: &[f32], sample_rate: u32) -> Option<[f64; 12]> {
    if samples.len() < KEY_FRAME * 2 || sample_rate == 0 {
        return None;
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(KEY_FRAME);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    let window: Vec<f32> = (0..KEY_FRAME)
        .map(|i| {
            let x = std::f32::consts::PI * 2.0 * i as f32 / KEY_FRAME as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect();

    let mut chroma = [0.0_f64; 12];
    // Hop by whole frames: key is a global property, so overlapping adds cost
    // without adding information.
    let mut start = 0;
    while start + KEY_FRAME <= samples.len() {
        let Some(chunk) = samples.get(start..start + KEY_FRAME) else { break };
        for (i, slot) in input.iter_mut().enumerate() {
            *slot = chunk.get(i).copied().unwrap_or(0.0) * window.get(i).copied().unwrap_or(0.0);
        }
        if fft.process(&mut input, &mut output).is_err() {
            break;
        }
        for (bin, value) in output.iter().enumerate() {
            if bin == 0 {
                continue;
            }
            let freq = f64::from(sample_rate) * bin as f64 / KEY_FRAME as f64;
            // Outside this range the window cannot resolve a semitone (low) or
            // the content is percussive rather than pitched (high).
            if !(90.0..=2500.0).contains(&freq) {
                continue;
            }
            // MIDI note number, then fold to a pitch class with A4 = 440 Hz.
            let midi = 69.0 + 12.0 * (freq / 440.0).log2();
            let class = (midi.round() as i64).rem_euclid(12) as usize;
            if let Some(slot) = chroma.get_mut(class) {
                // Energy rather than magnitude: it sharpens real partials
                // against the leakage skirts around them.
                let magnitude = f64::from(value.norm());
                *slot += magnitude * magnitude;
            }
        }
        start += KEY_FRAME;
    }
    Some(chroma)
}
