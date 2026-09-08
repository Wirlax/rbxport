//! What the interface reads to know where a deck is.
//!
//! The audio callback is the only writer of `position`, and it writes it once
//! per callback rather than per frame. Everything else here is written by the
//! control side and read by both. Atomics rather than a lock: the callback is
//! realtime and must never wait for a reader.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// One deck's shared state.
#[derive(Debug, Default)]
pub struct DeckClock {
    /// Frames of output produced since the track was loaded, at the device
    /// rate — the position of the playhead.
    position: AtomicU64,
    /// The track's length in device-rate frames, or 0 when it is not known.
    total: AtomicU64,
    /// Bumped on every load and every seek. Blocks in the ring carry the
    /// generation they were decoded under, so the callback can drop the ones
    /// that belong to where the playhead used to be.
    generation: AtomicU32,
    /// The device rate, which is the rate everything downstream of the
    /// resampler is in.
    sample_rate: AtomicU32,
    playing: AtomicBool,
    loaded: AtomicBool,
    /// The decode thread has pushed the last block it will push. The callback
    /// stops the deck when it has drained what is left.
    end_of_stream: AtomicBool,
}

/// A deck's state at one instant, for the tick the interface extrapolates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckSnapshot {
    pub position_frames: u64,
    pub total_frames: u64,
    pub generation: u32,
    pub sample_rate: u32,
    pub playing: bool,
    pub loaded: bool,
}

impl DeckClock {
    pub fn snapshot(&self) -> DeckSnapshot {
        DeckSnapshot {
            position_frames: self.position.load(Ordering::Relaxed),
            total_frames: self.total.load(Ordering::Relaxed),
            generation: self.generation.load(Ordering::Relaxed),
            sample_rate: self.sample_rate.load(Ordering::Relaxed),
            playing: self.playing.load(Ordering::Relaxed),
            loaded: self.loaded.load(Ordering::Relaxed),
        }
    }

    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    /// Written by the audio callback, once per callback.
    pub fn set_position(&self, frames: u64) {
        self.position.store(frames, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn set_total(&self, frames: u64) {
        self.total.store(frames, Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn set_sample_rate(&self, rate: u32) {
        self.sample_rate.store(rate, Ordering::Relaxed);
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }

    pub fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }

    pub fn set_loaded(&self, loaded: bool) {
        self.loaded.store(loaded, Ordering::Relaxed);
    }

    pub fn end_of_stream(&self) -> bool {
        self.end_of_stream.load(Ordering::Acquire)
    }

    pub fn set_end_of_stream(&self, ended: bool) {
        self.end_of_stream.store(ended, Ordering::Release);
    }

    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// Release ordering, and always before the first block of the new
    /// generation is pushed: the callback must never see a block from a
    /// generation the counter has not reached.
    pub fn bump_generation(&self) -> u32 {
        self.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_clock_is_at_the_start_and_stopped() {
        let clock = DeckClock::default();
        let snapshot = clock.snapshot();
        assert_eq!(snapshot.position_frames, 0);
        assert!(!snapshot.playing);
        assert!(!snapshot.loaded);
    }

    #[test]
    fn the_generation_moves_forward_on_every_bump() {
        let clock = DeckClock::default();
        assert_eq!(clock.bump_generation(), 1);
        assert_eq!(clock.bump_generation(), 2);
        assert_eq!(clock.generation(), 2);
    }
}
