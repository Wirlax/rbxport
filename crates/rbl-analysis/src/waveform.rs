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

/// Columns in a `PWAV` preview.
pub const PREVIEW_COLUMNS: usize = 400;
/// Columns in a `PWV2` preview.
pub const TINY_COLUMNS: usize = 100;
/// Columns in a `PWV4` colour preview.
pub const COLOUR_PREVIEW_COLUMNS: usize = 1200;

impl Waveform {
    /// The columns reduced to `count`, each taking the loudest of the
    /// columns it spans, so a transient survives the reduction the way it
    /// does on the CDJ's drawing.
    #[must_use]
    pub fn reduced(&self, count: usize) -> Vec<WaveformColumn> {
        if count == 0 || self.columns.is_empty() {
            return Vec::new();
        }
        (0..count)
            .map(|i| {
                let from = i * self.columns.len() / count;
                let to = ((i + 1) * self.columns.len() / count).max(from + 1).min(self.columns.len());
                self.columns[from..to].iter().fold(WaveformColumn::default(), |acc, c| WaveformColumn {
                    low: acc.low.max(c.low),
                    mid: acc.mid.max(c.mid),
                    high: acc.high.max(c.high),
                    peak: acc.peak.max(c.peak),
                })
            })
            .collect()
    }

    /// `PWAV`: 400 columns, height in the low five bits and whiteness — the
    /// high band — in the top three.
    #[must_use]
    pub fn pack_preview(&self) -> Vec<u8> {
        self.reduced(PREVIEW_COLUMNS).into_iter().map(blue_byte).collect()
    }

    /// `PWV2`: 100 columns of height alone, 0..=15 — no byte of any of 150
    /// reference files' `PWV2` exceeds 15 [OBS].
    #[must_use]
    pub fn pack_tiny(&self) -> Vec<u8> {
        self.reduced(TINY_COLUMNS).iter().map(|c| c.peak >> 4).collect()
    }

    /// `PWV3`: every column, packed as `PWAV` is.
    #[must_use]
    pub fn pack_detail(&self) -> Vec<u8> {
        self.columns.iter().copied().map(blue_byte).collect()
    }

    /// `PWV4`: 1,200 columns of six bytes — height 0..=255, two bytes whose
    /// meaning is [UNKNOWN] and are written zero, then the three colour
    /// channels 0..=127. The ranges are the reference files' [OBS: over 150
    /// `.EXT`s the first byte reaches 253, the last three 127].
    #[must_use]
    pub fn pack_colour_preview(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(COLOUR_PREVIEW_COLUMNS * 6);
        for c in self.reduced(COLOUR_PREVIEW_COLUMNS) {
            let (r, g, b) = rgb_of(c);
            out.extend_from_slice(&[c.peak, 0, 0, r >> 1, g >> 1, b >> 1]);
        }
        out
    }

    /// `PWV5`: every column as a big-endian word, `rrrgggbbbhhhhh00` — three
    /// bits each of red, green and blue, then five of height.
    #[must_use]
    pub fn pack_colour_detail(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.columns.len() * 2);
        for &c in &self.columns {
            let (r, g, b) = rgb_of(c);
            let word = (u16::from(r >> 5) << 13)
                | (u16::from(g >> 5) << 10)
                | (u16::from(b >> 5) << 7)
                | (u16::from(five_bits(c.peak)) << 2);
            out.extend_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// A magnitude 0..=255 as five bits.
fn five_bits(value: u8) -> u8 {
    value >> 3
}

/// The `PWAV`/`PWV3` byte: whiteness in the top three bits, height below.
fn blue_byte(c: WaveformColumn) -> u8 {
    ((c.high >> 5) << 5) | five_bits(c.peak)
}

/// A column's colour the way rekordbox's RGB waveform reads: bass blue,
/// mids amber, highs white. The bands are the ones in the column; the
/// mapping to channels is ours [UNKNOWN: rekordbox's own weights].
fn rgb_of(c: WaveformColumn) -> (u8, u8, u8) {
    let amber_green = u8::try_from(u16::from(c.mid) * 3 / 5).unwrap_or(u8::MAX);
    (c.mid.max(c.high), amber_green.max(c.high), c.low.max(c.high))
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod pack_tests {
    use super::*;

    fn wave(columns: &[(u8, u8, u8, u8)]) -> Waveform {
        Waveform {
            columns: columns.iter().map(|&(low, mid, high, peak)| WaveformColumn { low, mid, high, peak }).collect(),
            columns_per_sec: COLUMNS_PER_SEC,
        }
    }

    #[test]
    fn reduction_keeps_the_loudest_column_of_each_span() {
        let w = wave(&[(0, 0, 0, 10), (0, 0, 0, 200), (0, 0, 0, 30), (0, 0, 0, 40)]);
        let r = w.reduced(2);
        assert_eq!(r.iter().map(|c| c.peak).collect::<Vec<_>>(), vec![200, 40]);
        assert!(w.reduced(0).is_empty());
        assert_eq!(w.reduced(8).len(), 8, "more buckets than columns still yields every bucket");
    }

    #[test]
    fn previews_have_rekordbox_s_column_counts_and_bit_layouts() {
        let w = wave(&[(255, 0, 0, 255), (0, 255, 0, 128), (0, 0, 255, 8)]);
        assert_eq!(w.pack_preview().len(), PREVIEW_COLUMNS);
        assert_eq!(w.pack_tiny().len(), TINY_COLUMNS);
        assert_eq!(w.pack_colour_preview().len(), COLOUR_PREVIEW_COLUMNS * 6);
        assert_eq!(w.pack_detail().len(), 3);
        assert_eq!(w.pack_colour_detail().len(), 6);
        // Height fills five bits; whiteness three; nothing overflows.
        assert_eq!(w.pack_detail()[0], 0b000_11111);
        assert_eq!(w.pack_detail()[2], 0b111_00001);
        assert!(w.pack_tiny().iter().all(|&b| b < 16));
        // The bass column is blue, the mid amber, the high white.
        let words: Vec<u16> = w.pack_colour_detail().chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        let rgb = |v: u16| ((v >> 13) & 7, (v >> 10) & 7, (v >> 7) & 7);
        assert_eq!(rgb(words[0]), (0, 0, 7));
        assert_eq!(rgb(words[1]), (7, 4, 0));
        assert_eq!(rgb(words[2]), (7, 7, 7));
        assert!(words.iter().all(|v| v.trailing_zeros() >= 2), "the low two bits are unused");
        let colour = w.pack_colour_preview();
        assert!(colour.chunks_exact(6).all(|c| c[3] < 128 && c[4] < 128 && c[5] < 128), "colour channels stay within seven bits");
        assert_eq!(colour[0], 255, "height uses the whole byte");
    }
}
