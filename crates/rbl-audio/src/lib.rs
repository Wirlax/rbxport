//! Decoding for analysis.
//!
//! Analysis wants one mono signal at a known rate, not the file's own layout.
//! Decoding is the expensive step, so it happens once and every later stage
//! reads the same buffer.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "sample counts and durations convert between usize and f64 throughout decoding"
)]

use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("unsupported or unreadable audio: {0}")]
    Unsupported(String),
    #[error("no audio track in the file")]
    NoTrack,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AudioError>;

/// Mono audio at a known sample rate.
#[derive(Debug, Clone)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// Channels in the source, before the downmix.
    pub source_channels: u16,
}

impl Audio {
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / f64::from(self.sample_rate)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Decodes a file to mono `f32`.
///
/// `max_secs` bounds the work: tempo and key are stable well before a whole
/// long mix is decoded, and an unbounded decode is how a 2-hour file becomes a
/// memory problem.
pub fn decode_mono(path: &Path, max_secs: Option<f64>) -> Result<Audio> {
    let file = std::fs::File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), symphonia::core::io::MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| AudioError::Unsupported(e.to_string()))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or(AudioError::NoTrack)?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| AudioError::Unsupported(e.to_string()))?;

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut source_channels = track
        .codec_params
        .channels
        .map_or(2, |c| u16::try_from(c.count()).unwrap_or(2));

    let cap = max_secs.map(|s| (s * f64::from(sample_rate)) as usize);
    let mut samples: Vec<f32> = Vec::with_capacity(cap.unwrap_or(0).min(1 << 24));
    let mut buffer: Option<SampleBuffer<f32>> = None;

    // `next_packet` reports end of stream as an error, so this reads until it
    // fails rather than testing a separate condition.
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let frames = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet mid-file should not discard what we already have.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        };

        let spec = *frames.spec();
        sample_rate = spec.rate;
        source_channels = u16::try_from(spec.channels.count()).unwrap_or(2);

        let interleaved =
            buffer.get_or_insert_with(|| SampleBuffer::new(frames.capacity() as u64, spec));
        interleaved.copy_interleaved_ref(frames);

        let channels = spec.channels.count().max(1);
        for frame in interleaved.samples().chunks(channels) {
            let sum: f32 = frame.iter().sum();
            samples.push(sum / channels as f32);
        }

        if let Some(cap) = cap {
            if samples.len() >= cap {
                samples.truncate(cap);
                break;
            }
        }
    }

    if samples.is_empty() {
        return Err(AudioError::Unsupported("decoded no samples".into()));
    }

    Ok(Audio { samples, sample_rate, source_channels })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::pedantic)]
mod tests {
    use super::*;

    /// Writes a 16-bit PCM WAV, so the test needs no fixture file.
    fn write_wav(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) {
        let bits = 16_u16;
        let block_align = channels * bits / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_len = u32::try_from(samples.len() * 2).unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn decodes_a_mono_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let samples: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 44_100.0).sin() * 0.5)
            .collect();
        write_wav(&path, 44_100, 1, &samples);

        let audio = decode_mono(&path, None).unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.source_channels, 1);
        assert!((audio.duration_secs() - 1.0).abs() < 0.01);
        // The tone should survive round-tripping through 16-bit.
        let peak = audio.samples.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "peak was {peak}");
    }

    #[test]
    fn downmixes_stereo_to_mono() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        // Left +0.5, right -0.5 must cancel to silence.
        let mut samples = Vec::new();
        for _ in 0..1000 {
            samples.push(0.5);
            samples.push(-0.5);
        }
        write_wav(&path, 44_100, 2, &samples);

        let audio = decode_mono(&path, None).unwrap();
        assert_eq!(audio.source_channels, 2);
        assert_eq!(audio.samples.len(), 1000);
        let peak = audio.samples.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
        assert!(peak < 0.01, "opposite channels should cancel, peak was {peak}");
    }

    #[test]
    fn respects_the_duration_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        let samples: Vec<f32> = (0..44_100 * 10).map(|i| (i as f32 / 1000.0).sin()).collect();
        write_wav(&path, 44_100, 1, &samples);

        let audio = decode_mono(&path, Some(2.0)).unwrap();
        assert!(audio.duration_secs() <= 2.05, "got {}", audio.duration_secs());
        assert!(audio.duration_secs() >= 1.95);
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(decode_mono(Path::new("/definitely/not/here.wav"), None).is_err());
    }

    #[test]
    fn a_non_audio_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-audio.wav");
        std::fs::write(&path, b"this is not a wav file at all").unwrap();
        assert!(decode_mono(&path, None).is_err());
    }
}
