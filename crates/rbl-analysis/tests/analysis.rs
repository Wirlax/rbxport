//! Analysis correctness against synthesised signals with known answers.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_analysis::{analyse, key::detect_key, onset::onset_envelope, tempo::detect_tempo};

const SR: u32 = 44_100;

/// A click track: one short burst per beat at a known tempo.
fn click_track(bpm: f64, secs: f64) -> Vec<f32> {
    let total = (secs * f64::from(SR)) as usize;
    let period = (60.0 / bpm * f64::from(SR)) as usize;
    let mut out = vec![0.0_f32; total];
    let mut at = 0;
    while at < total {
        // 5 ms decaying burst of noise-ish content.
        for i in 0..(SR as usize / 200) {
            if at + i >= total {
                break;
            }
            let decay = 1.0 - i as f32 / (SR as f32 / 200.0);
            let phase = i as f32 * 0.7;
            out[at + i] += phase.sin() * decay * 0.8;
        }
        at += period;
    }
    out
}

/// A click track whose off-beats are quieter, so half-time correlates too.
///
/// This is the shape that makes a detector read half-time: the loud clicks
/// alone are a perfectly good pulse, and on their own score higher than the
/// full one.
fn accented_click_track(bpm: f64, secs: f64, off_beat: f32) -> Vec<f32> {
    let total = (secs * f64::from(SR)) as usize;
    let period = (60.0 / bpm * f64::from(SR)) as usize;
    let mut out = vec![0.0_f32; total];
    let mut at = 0;
    let mut beat = 0;
    while at < total {
        let level = if beat % 2 == 0 { 0.8 } else { 0.8 * off_beat };
        for i in 0..(SR as usize / 200) {
            if at + i >= total {
                break;
            }
            let decay = 1.0 - i as f32 / (SR as f32 / 200.0);
            let phase = i as f32 * 0.7;
            out[at + i] += phase.sin() * decay * level;
        }
        at += period;
        beat += 1;
    }
    out
}

/// A sine at a given frequency.
fn tone(freq: f64, secs: f64) -> Vec<f32> {
    let total = (secs * f64::from(SR)) as usize;
    (0..total)
        .map(|i| {
            (i as f64 * 2.0 * std::f64::consts::PI * freq / f64::from(SR)).sin() as f32 * 0.5
        })
        .collect()
}

/// A chord, for key detection.
fn chord(freqs: &[f64], secs: f64) -> Vec<f32> {
    let total = (secs * f64::from(SR)) as usize;
    (0..total)
        .map(|i| {
            let t = i as f64 / f64::from(SR);
            let sum: f64 = freqs
                .iter()
                .map(|f| (t * 2.0 * std::f64::consts::PI * f).sin())
                .sum();
            (sum / freqs.len() as f64) as f32 * 0.5
        })
        .collect()
}

#[test]
fn finds_the_tempo_of_a_click_track() {
    for bpm in [120.0, 128.0, 140.0, 174.0] {
        let audio = click_track(bpm, 20.0);
        let onsets = onset_envelope(&audio, SR);
        let result = detect_tempo(&onsets, SR);
        // Allow an octave error, then require the tempo itself to be close.
        let folded = rbl_analysis::tempo::nearest_octave(result.bpm, bpm);
        assert!(
            (folded - bpm).abs() < 1.0,
            "expected ~{bpm}, got {} (folded {folded})",
            result.bpm
        );
    }
}

#[test]
fn an_accented_pulse_is_not_read_as_half_time() {
    // The candidate's support at twice and three times its lag is what tells
    // the true beat from its slower relatives. That term once read scores that
    // had not been computed yet, so it did nothing; this is what it is for.
    for (bpm, off_beat) in [(160.0, 0.35), (150.0, 0.3), (128.0, 0.4)] {
        let audio = accented_click_track(bpm, 24.0, off_beat);
        let onsets = onset_envelope(&audio, SR);
        let result = detect_tempo(&onsets, SR);
        assert!(
            (result.bpm - bpm).abs() < 1.0,
            "expected {bpm}, got {} — half-time would be {}",
            result.bpm,
            bpm / 2.0
        );
    }
}

#[test]
fn the_beat_grid_lands_on_the_clicks() {
    let bpm = 128.0;
    let audio = click_track(bpm, 20.0);
    let onsets = onset_envelope(&audio, SR);
    let result = detect_tempo(&onsets, SR);
    assert!(!result.beats.is_empty(), "grid should not be empty");

    let expected_period_ms = 60_000.0 / bpm;
    // Consecutive beats must be one period apart (allowing the detected octave).
    if result.beats.len() >= 3 {
        let d = f64::from(result.beats[1].time_ms) - f64::from(result.beats[0].time_ms);
        let ratio = d / expected_period_ms;
        let nearest = [0.25, 1.0 / 3.0, 0.5, 1.0, 2.0, 3.0, 4.0]
            .into_iter()
            .min_by(|a, b| (a - ratio).abs().partial_cmp(&(b - ratio).abs()).unwrap())
            .unwrap();
        assert!((ratio - nearest).abs() < 0.05, "beat spacing {d}ms vs period {expected_period_ms}ms");
    }
    // Beat numbers cycle 1..4.
    assert_eq!(result.beats[0].beat_number, 1);
    if result.beats.len() > 4 {
        assert_eq!(result.beats[4].beat_number, 1);
    }
}

#[test]
fn silence_yields_no_tempo_rather_than_a_wrong_one() {
    let silence = vec![0.0_f32; SR as usize * 5];
    let onsets = onset_envelope(&silence, SR);
    let result = detect_tempo(&onsets, SR);
    assert!(result.confidence < 0.5, "silence should not look confident");
}

#[test]
fn very_short_audio_does_not_panic() {
    for len in [0_usize, 1, 100, 1023] {
        let audio = vec![0.1_f32; len];
        let onsets = onset_envelope(&audio, SR);
        let result = detect_tempo(&onsets, SR);
        assert_eq!(result.beats.len(), 0);
        let _ = analyse(&audio, SR);
    }
}

#[test]
fn detects_the_key_of_a_scale() {
    // A bare triad is genuinely ambiguous — A-C-E also sits inside F major —
    // so establish the key the way music does, with the full scale.
    // A natural minor: A B C D E F G A.
    let scale = [220.0, 246.94, 261.63, 293.66, 329.63, 349.23, 392.00, 440.0];
    let mut audio = Vec::new();
    for &f in &scale {
        audio.extend(chord(&[f], 0.4));
    }
    // Repeat so the chromagram has enough material.
    let once = audio.clone();
    for _ in 0..3 {
        audio.extend_from_slice(&once);
    }

    let key = detect_key(&audio, SR).expect("should find a key");
    // A minor and C major share all seven notes; either is a correct reading of
    // this material, and rekordbox itself reports one or the other for such tracks.
    assert!(
        key.name == "Am" || key.name == "C",
        "expected Am or its relative major C, got {}",
        key.name
    );
}

#[test]
fn camelot_codes_follow_the_wheel() {
    use rbl_analysis::key::MusicalKey;
    let am = MusicalKey { name: "Am".into(), tonic: 9, minor: true };
    assert_eq!(am.camelot(), "8A");
    let c = MusicalKey { name: "C".into(), tonic: 0, minor: false };
    assert_eq!(c.camelot(), "8B");
    let abm = MusicalKey { name: "Abm".into(), tonic: 8, minor: true };
    assert_eq!(abm.camelot(), "1A");
}

#[test]
fn a_pure_tone_has_no_meaningful_key_but_does_not_panic() {
    let audio = tone(440.0, 3.0);
    // A single pitch class may still correlate with something; the requirement
    // is only that it does not panic or return nonsense.
    if let Some(key) = detect_key(&audio, SR) {
        assert!(!key.name.is_empty());
    }
}

#[test]
fn the_waveform_has_one_column_per_150th_of_a_second() {
    let audio = tone(440.0, 2.0);
    let result = analyse(&audio, SR);
    let expected = (2.0 * 150.0) as usize;
    let got = result.waveform.columns.len();
    assert!(
        got.abs_diff(expected) <= 2,
        "expected about {expected} columns for 2s, got {got}"
    );
}

#[test]
fn the_waveform_separates_bands() {
    // A low tone should put energy in `low`, a high tone in `high`.
    let low = analyse(&tone(60.0, 2.0), SR).waveform;
    let high = analyse(&tone(8000.0, 2.0), SR).waveform;

    let mean = |cols: &[rbl_analysis::WaveformColumn], f: fn(&rbl_analysis::WaveformColumn) -> u8| {
        // Skip the first columns while the filters settle.
        let tail = &cols[cols.len() / 4..];
        tail.iter().map(|c| u32::from(f(c))).sum::<u32>() / tail.len().max(1) as u32
    };

    assert!(mean(&low.columns, |c| c.low) > mean(&low.columns, |c| c.high),
        "a 60 Hz tone should be mostly low band");
    assert!(mean(&high.columns, |c| c.high) > mean(&high.columns, |c| c.low),
        "an 8 kHz tone should be mostly high band");
}

#[test]
fn peak_and_rms_are_measured() {
    let audio = tone(440.0, 1.0); // amplitude 0.5
    let result = analyse(&audio, SR);
    assert!((result.peak - 0.5).abs() < 0.01, "peak {}", result.peak);
    // RMS of a sine is amplitude / sqrt(2).
    let expected_rms = 0.5 / std::f32::consts::SQRT_2;
    assert!((result.rms - expected_rms).abs() < 0.01, "rms {}", result.rms);
}

#[test]
fn phrase_and_vocal_detection_report_that_they_are_unimplemented() {
    use rbl_analysis::phrase::{PhraseAnalyzer, Unimplemented as Phrases};
    use rbl_analysis::vocal::{VocalDetector, Unimplemented as Vocals};
    let audio = tone(440.0, 1.0);
    // These must return None rather than invent structure that a DJ would see.
    assert!(Phrases.phrases(&audio, SR).is_none());
    assert!(Vocals.vocals(&audio, SR).is_none());
}
