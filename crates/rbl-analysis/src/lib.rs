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

pub mod downbeat;
pub mod key;
pub mod onset;
pub mod phrase;
pub mod tempo;
pub mod vocal;
pub mod waveform;

pub use key::{detect_key, MusicalKey};
pub use tempo::{detect_tempo, Beat, Segment, TempoResult};
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

/// Beats before the first downbeat, in `0..4`, for a grid whose downbeat is
/// nearest `downbeat_secs`.
fn phase_for(segments: &[tempo::Segment], downbeat_secs: f64) -> usize {
    let unnumbered = tempo::beats_of(segments, 0);
    let index = unnumbered
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let da = (f64::from(a.1.time_ms) / 1000.0 - downbeat_secs).abs();
            let db = (f64::from(b.1.time_ms) / 1000.0 - downbeat_secs).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(0, |(i, _)| i);
    (4 - index % 4) % 4
}

/// Runs the full analysis over mono audio.
pub fn analyse(samples: &[f32], sample_rate: u32) -> Analysis {
    let onsets = onset::onset_envelope(samples, sample_rate);
    let mut tempo = tempo::detect_tempo(&onsets);
    // The grid comes back numbered from its first beat, and possibly on the
    // off-beat. The downbeat stage looks at the music's structure and says
    // both; the grid is moved if it has to be and renumbered so that 1 is
    // the downbeat.
    let beat_secs: Vec<f64> = tempo.beats.iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
    let grid = downbeat::grid_phase(samples, sample_rate, &beat_secs);
    if grid.half_beat_off {
        for segment in &mut tempo.segments {
            *segment = segment.shifted_half_beat();
        }
        tempo.first_beat_secs = tempo.segments.first().map_or(0.0, tempo::Segment::start_secs);
    }
    // The beat nearest the chosen downbeat is beat 1, and the count runs
    // on from there through every segment. Then each segment long enough
    // to have phrases of its own is asked again, on its own beats only: a
    // DJ edit's two halves are two pieces of music, and a bar count carried
    // across a tempo change that landed a beat off would misnumber the
    // whole second half.
    let mut beats = tempo::beats_of(&tempo.segments, phase_for(&tempo.segments, grid.downbeat_secs));
    let mut offset = 0usize;
    for segment in &tempo.segments {
        let count = segment.beats();
        if count >= downbeat::MIN_BEATS_FOR_OWN_PHASE && tempo.segments.len() > 1 {
            let own: Vec<f64> = beats[offset..offset + count].iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
            let own_grid = downbeat::grid_phase(samples, sample_rate, &own);
            // A half-beat verdict is only taken from the whole track above;
            // here only the bar position is used.
            let downbeat = own
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1 - own_grid.downbeat_secs).abs().partial_cmp(&(b.1 - own_grid.downbeat_secs).abs()).unwrap_or(std::cmp::Ordering::Equal))
                .map_or(0, |(i, _)| i);
            for (i, beat) in beats[offset..offset + count].iter_mut().enumerate() {
                beat.beat_number = u16::try_from((i + 4 - downbeat % 4) % 4 + 1).unwrap_or(1);
            }
        }
        offset += count;
    }
    tempo.beats = beats;
    // The key rules may read the bass on or between beats and after phrase
    // starts, so they are given the grid.
    let key_grid = key::KeyGrid {
        beats: tempo.beats.iter().map(|b| (f64::from(b.time_ms) / 1000.0, b.beat_number)).collect(),
        phrase_starts: grid.phrase_starts.clone(),
    };
    let key = key::detect_key_with(samples, sample_rate, key::KeyOptions::default(), key::DEFAULT_RULES, &key_grid)
        .map(|report| report.key);
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
