//! Three-band waveform, matching what rekordbox draws.
//!
//! Each column holds low, mid and high magnitudes: low is the kick, mid the
//! body, high the hats. This is the same decomposition the ANLZ colour
//! waveforms encode, so the columns map onto `PWV4`/`PWV5` directly.

/// Columns per second, matching rekordbox's detail waveform resolution.
pub const COLUMNS_PER_SEC: f64 = 150.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaveformColumn {
    pub low: u8,
    pub mid: u8,
    pub high: u8,
    /// Overall peak for the column, which drives the height drawn.
    pub peak: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Waveform {
    pub columns: Vec<WaveformColumn>,
    pub columns_per_sec: f64,
}

/// One-pole filters, enough to separate three bands cheaply and stably.
struct Bands {
    low_state: f32,
    high_state: f32,
    low_coeff: f32,
    high_coeff: f32,
}

impl Bands {
    fn new(sample_rate: f32) -> Self {
        // Crossovers at 200 Hz and 2 kHz, as the plan specifies.
        let coeff = |cutoff: f32| {
            let x = (-2.0 * std::f32::consts::PI * cutoff / sample_rate).exp();
            x.clamp(0.0, 0.9999)
        };
        Self { low_state: 0.0, high_state: 0.0, low_coeff: coeff(200.0), high_coeff: coeff(2000.0) }
    }

    /// Splits one sample into (low, mid, high).
    fn split(&mut self, sample: f32) -> (f32, f32, f32) {
        self.low_state = sample * (1.0 - self.low_coeff) + self.low_state * self.low_coeff;
        let low = self.low_state;
        self.high_state = sample * (1.0 - self.high_coeff) + self.high_state * self.high_coeff;
        let below_2k = self.high_state;
        let high = sample - below_2k;
        let mid = below_2k - low;
        (low, mid, high)
    }
}

/// Computes the waveform.
pub fn compute(samples: &[f32], sample_rate: u32) -> Waveform {
    if samples.is_empty() || sample_rate == 0 {
        return Waveform { columns: Vec::new(), columns_per_sec: COLUMNS_PER_SEC };
    }
    let per_column = (f64::from(sample_rate) / COLUMNS_PER_SEC).max(1.0) as usize;
    let column_count = samples.len().div_ceil(per_column);
    let mut columns = Vec::with_capacity(column_count);
    let mut bands = Bands::new(sample_rate as f32);

    for chunk in samples.chunks(per_column) {
        let (mut low, mut mid, mut high, mut peak) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for &sample in chunk {
            let (l, m, h) = bands.split(sample);
            low = low.max(l.abs());
            mid = mid.max(m.abs());
            high = high.max(h.abs());
            peak = peak.max(sample.abs());
        }
        let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        columns.push(WaveformColumn {
            low: to_u8(low),
            mid: to_u8(mid),
            high: to_u8(high),
            peak: to_u8(peak),
        });
    }

    Waveform { columns, columns_per_sec: COLUMNS_PER_SEC }
}
