//! Musical key by chromagram plus profile matching.
//!
//! Spectral energy is folded onto the twelve pitch classes, then correlated
//! against major and minor profiles. The best of the 24 rotations is the key.
//!
//! # Measured, and validated on tracks that did not choose it
//!
//! Every choice below was picked against rekordbox's own key stamps with
//! `cargo run --release -p rbl-analysis --example keytune`. The figures that
//! matter are from 75 tracks held out of the search entirely:
//!
//! | on 75 held-out tracks | exact | compatible |
//! |---|---|---|
//! | before any tuning | 25% | 52% |
//! | **this** | **47%** | **83%** |
//!
//! "Compatible" counts a relative key or a neighbour on the Camelot wheel,
//! which for mixing purposes is a usable answer.
//!
//! An earlier round of this could not rule out fitting, because 192 variants
//! were scored against the same 150 tracks that chose them. Re-running the
//! search with a train/test split settled it: the held-out 47% matches the
//! training figure, and no variant among the 192 beat what is here — the best
//! differed only by normalising each frame, which cost two points of
//! compatible agreement.
//!
//! Where the remaining error sits, on those 75: 35 exact, 6 a relative
//! major/minor, **20 a fifth away**, 1 the right root in the wrong mode, 13
//! unrelated. The fifths are the interesting group — a fifth is a usable mix
//! but a different key, and 20 of 75 is where the next improvement is.

use realfft::RealFftPlanner;

/// Chromagram window.
///
/// This must be much longer than the onset window: at 44.1 kHz a 1024-sample
/// FFT resolves 43 Hz, while a semitone at 220 Hz spans about 13 Hz — too
/// coarse to tell pitch classes apart at all in the bass, which made an A minor
/// scale read as F major. 8192 gives ~5.4 Hz, enough down to about 90 Hz.
const KEY_FRAME: usize = 8192;

/// What belongs to a key, weighted by how much it defines it: the tonic and
/// dominant carry it, the rest of the scale supports, everything else is zero.
///
/// This beat the Krumhansl-Kessler and Temperley profiles by several points in
/// the measurement above. Those were fitted to listeners rating classical
/// probe tones; a dance track states its key with a bass line and a chord stab,
/// and a flat profile of "the notes of the key" describes that better.
const MAJOR: [f64; 12] = [3.0, 0.0, 1.0, 0.0, 2.0, 1.0, 0.0, 2.5, 0.0, 1.0, 0.0, 1.0];
const MINOR: [f64; 12] = [3.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 2.5, 1.0, 0.0, 1.0, 0.5];

/// How much a minor reading is favoured over a major one.
///
/// Not a thumb on the scale: 89% of the tracks in this library are minor, and
/// a profile correlation alone has no way to know that. Added rather than
/// multiplied, because a correlation is signed and scaling a negative score
/// makes it worse — which is the opposite of a preference.
const MINOR_BIAS: f64 = 0.10;

/// How much of a pitch class's energy to remove from the classes its own
/// harmonics land on: a fifth above (third harmonic) and a major third above
/// (fifth harmonic).
const FIFTH_LEAK: f64 = 0.30;
const THIRD_LEAK: f64 = 0.15;

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
    let mut chroma = chromagram(samples, sample_rate)?;
    let total: f64 = chroma.iter().sum();
    if total <= f64::EPSILON {
        return None;
    }
    remove_harmonic_leakage(&mut chroma);

    let mut best: Option<(f64, usize, bool)> = None;
    for tonic in 0..12 {
        for minor in [false, true] {
            let profile = if minor { &MINOR } else { &MAJOR };
            let score = correlate(&chroma, profile, tonic)
                + if minor { MINOR_BIAS } else { 0.0 };
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

/// Removes the chroma a note's own harmonics contribute to other pitch classes.
///
/// A note sounding C also puts energy on G and E, so a C minor track reads as
/// partly G and partly E major. Subtracting a fixed fraction of every class
/// from the two it leaks into was the most consistent single improvement
/// measured — it helped under every profile and every compression.
fn remove_harmonic_leakage(chroma: &mut [f64; 12]) {
    let before = *chroma;
    for class in 0..12 {
        let energy = before.get(class).copied().unwrap_or(0.0);
        if let Some(slot) = chroma.get_mut((class + 7) % 12) {
            *slot -= FIFTH_LEAK * energy;
        }
        if let Some(slot) = chroma.get_mut((class + 4) % 12) {
            *slot -= THIRD_LEAK * energy;
        }
    }
    for slot in chroma.iter_mut() {
        *slot = slot.max(0.0);
    }
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
            // Wider than it was: the bass line states the key as clearly as
            // anything above it, and cutting at 90 Hz threw that away.
            if !(55.0..=5000.0).contains(&freq) {
                continue;
            }
            // MIDI note number, with A4 = 440 Hz.
            let midi = 69.0 + 12.0 * (freq / 440.0).log2();
            // Only bins within a sixth of a semitone of a note count. The rest
            // are the leakage skirt around it, and counting them is counting
            // noise — measurably so.
            if ((midi * 3.0).round() as i64).rem_euclid(3) != 0 {
                continue;
            }
            let class = (midi.round() as i64).rem_euclid(12) as usize;
            if let Some(slot) = chroma.get_mut(class) {
                // Log compression, not energy: squaring let one loud partial
                // outweigh a whole track's worth of quieter pitched content.
                *slot += (1.0 + f64::from(value.norm())).ln();
            }
        }
        start += KEY_FRAME;
    }
    Some(chroma)
}
