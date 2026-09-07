//! Onset detection by spectral flux.
//!
//! Percussive onsets show up as a sudden rise in energy across many frequency
//! bins at once. Summing only the *increases* between consecutive spectra picks
//! those out while ignoring steady tones, which is what makes it work on dance
//! music where the kick dominates.

use realfft::RealFftPlanner;

/// Analysis frame size. At 44.1 kHz this is ~23 ms, short enough to place a
/// kick precisely and long enough for a usable spectrum.
pub const FRAME: usize = 1024;
/// Frames advance by this many samples: ~5.8 ms at 44.1 kHz.
pub const HOP: usize = 256;

/// The onset strength signal, one value per hop.
#[derive(Debug, Clone)]
pub struct OnsetEnvelope {
    pub values: Vec<f32>,
    /// Envelope samples per second.
    pub rate: f64,
}

impl OnsetEnvelope {
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// Seconds represented by `n` envelope samples.
    pub fn seconds(&self, n: usize) -> f64 {
        if self.rate <= 0.0 { 0.0 } else { n as f64 / self.rate }
    }
}

/// Computes the spectral-flux onset envelope.
pub fn onset_envelope(samples: &[f32], sample_rate: u32) -> OnsetEnvelope {
    let rate = f64::from(sample_rate) / HOP as f64;
    if samples.len() < FRAME || sample_rate == 0 {
        return OnsetEnvelope { values: Vec::new(), rate };
    }

    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    // Hann window: without it, frame edges produce spectral splatter that looks
    // like an onset on every frame.
    let window: Vec<f32> = (0..FRAME)
        .map(|i| {
            let x = std::f32::consts::PI * 2.0 * i as f32 / FRAME as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect();

    let frames = (samples.len().saturating_sub(FRAME)) / HOP + 1;
    let mut values = Vec::with_capacity(frames);
    let mut previous = vec![0.0_f32; output.len()];

    for frame in 0..frames {
        let start = frame * HOP;
        let Some(chunk) = samples.get(start..start + FRAME) else { break };
        for (i, slot) in input.iter_mut().enumerate() {
            *slot = chunk.get(i).copied().unwrap_or(0.0) * window.get(i).copied().unwrap_or(0.0);
        }
        if fft.process(&mut input, &mut output).is_err() {
            break;
        }

        let mut flux = 0.0_f32;
        for (i, bin) in output.iter().enumerate() {
            let magnitude = bin.norm();
            let rise = magnitude - previous.get(i).copied().unwrap_or(0.0);
            // Only increases count: a decay is not an onset.
            if rise > 0.0 {
                flux += rise;
            }
            if let Some(slot) = previous.get_mut(i) {
                *slot = magnitude;
            }
        }
        values.push(flux);
    }

    normalise(&mut values);
    OnsetEnvelope { values, rate }
}

/// Subtracts a local mean and clips at zero, which removes slow loudness drift
/// so a quiet intro and a loud drop contribute equally to tempo estimation.
fn normalise(values: &mut [f32]) {
    const WINDOW: usize = 16;
    if values.len() <= WINDOW {
        return;
    }
    let original = values.to_vec();
    for (i, value) in values.iter_mut().enumerate() {
        let lo = i.saturating_sub(WINDOW);
        let hi = (i + WINDOW).min(original.len());
        let slice = original.get(lo..hi).unwrap_or(&[]);
        if slice.is_empty() {
            continue;
        }
        let mean: f32 = slice.iter().sum::<f32>() / slice.len() as f32;
        *value = (*value - mean).max(0.0);
    }
    let peak = values.iter().fold(0.0_f32, |a, &v| a.max(v));
    if peak > 0.0 {
        for value in values.iter_mut() {
            *value /= peak;
        }
    }
}
