//! Track analysis: tempo, beat grid, key and waveforms.
//!
//! Everything here is offline and deterministic — the same audio always yields
//! the same result — so a analysis can be compared against rekordbox's own
//! stamps as a regression test.
//!
//! Placeholders: phrase detection (`PSSI`) and vocal detection (`PVDI`) are
//! declared as traits with unimplemented stubs. They are deliberately not
//! guessed; see [`phrase`] and [`vocal`].

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "DSP converts freely between sample counts and float time; every such cast is bounded by the buffer length"
)]

pub mod key;
pub mod onset;
pub mod phrase;
pub mod tempo;
pub mod vocal;
pub mod waveform;

pub use key::{detect_key, MusicalKey};
pub use tempo::{detect_tempo, Beat, TempoResult};
pub use waveform::{Waveform, WaveformColumn};

/// Everything one pass over a track produces.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub tempo: TempoResult,
    pub key: Option<MusicalKey>,
    pub waveform: Waveform,
    /// Peak sample magnitude, for the gain rekordbox stores in `djmdMixerParam`.
    pub peak: f32,
    /// RMS over the whole track.
    pub rms: f32,
}

/// Runs the full analysis over mono audio.
pub fn analyse(samples: &[f32], sample_rate: u32) -> Analysis {
    let onsets = onset::onset_envelope(samples, sample_rate);
    let tempo = tempo::detect_tempo(&onsets, sample_rate);
    let key = key::detect_key(samples, sample_rate);
    let waveform = waveform::compute(samples, sample_rate);

    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f64;
    for &s in samples {
        peak = peak.max(s.abs());
        sum_squares += f64::from(s) * f64::from(s);
    }
    let rms = if samples.is_empty() {
        0.0
    } else {
        (sum_squares / samples.len() as f64).sqrt() as f32
    };

    Analysis { tempo, key, waveform, peak, rms }
}
