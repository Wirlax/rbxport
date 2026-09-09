//! The playback engine: two decks, one audio device, one clock.
//!
//! Playback is here rather than on an `<audio>` element because the 2-player
//! view rekordbox has needs phase-locked beat sync, key sync and audible
//! drag-scrub, and a media element has a primitive for none of them. See
//! `docs/player-engine.md`.
//!
//! # Threads
//!
//! - **The audio callback**, owned by the device. Reads each deck's ring,
//!   sums, writes the buffer, publishes each deck's position. It allocates
//!   nothing, locks nothing, and calls into nothing that can block; where it
//!   finds no audio it writes silence rather than whatever was in the buffer.
//! - **One decode thread per deck.** All the file I/O, demuxing, decoding and
//!   resampling. Blocks it produces carry the generation they belong to, which
//!   is what makes a seek exact rather than approximate.
//! - **The control side** — a Tauri command, or a test — which only ever sends
//!   messages and reads atomics. Nothing it does can block audio.
//!
//! Two decks, named rather than indexed. Going to four would rework the mixer
//! and the event payload; that is accepted (`docs/player-engine.md` §4).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "frame counts and sample rates convert between integers and f64 throughout"
)]

mod block;
mod clock;
mod deck;
mod decode;
mod fade;
mod scrub;
mod sink;
mod smooth;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use rtrb::Consumer;

pub use clock::{DeckClock, DeckSnapshot};
pub use fade::FADE_FRAMES;
pub use sink::{CpalSink, NullSink, Render, Sink};

use block::{Block, RING_BLOCKS};
use fade::Ramp;
use smooth::Smoothed;

#[derive(Debug, thiserror::Error)]
pub enum DeckError {
    #[error("there is no audio output device")]
    NoDevice,
    #[error("the audio device could not be used: {0}")]
    Device(String),
    #[error("unsupported or unreadable audio: {0}")]
    Decode(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DeckError>;

/// Which deck. Named rather than indexed, as the mixer and the interface are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Deck {
    A,
    B,
}

impl Deck {
    pub fn name(self) -> &'static str {
        match self {
            Deck::A => "a",
            Deck::B => "b",
        }
    }

    fn index(self) -> usize {
        match self {
            Deck::A => 0,
            Deck::B => 1,
        }
    }

    pub const ALL: [Deck; 2] = [Deck::A, Deck::B];
}

/// What the engine tells the interface about, outside the position tick.
#[derive(Debug, Clone)]
pub enum DeckEvent {
    Loaded { deck: Deck, total_frames: u64, sample_rate: u32 },
    Error { deck: Deck, message: String },
}

/// Where those go. Called from a decode thread, never from the audio callback.
pub type EventSink = Arc<dyn Fn(DeckEvent) + Send + Sync>;

/// Both decks at one instant, which is what one tick of the clock carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub a: DeckSnapshot,
    pub b: DeckSnapshot,
    pub sample_rate: u32,
}

impl Snapshot {
    pub fn any_playing(&self) -> bool {
        self.a.playing || self.b.playing
    }
}

/// The master level, and the peaks that came out of it.
///
/// One `AtomicU32` a value, holding an `f32`'s bits: the callback writes them
/// and the interface reads them, and neither ever blocks the other.
#[derive(Debug)]
pub struct Master {
    gain: AtomicU32,
    peak_left: AtomicU32,
    peak_right: AtomicU32,
    /// The device's rate, so the callback can smooth a fader in real time.
    ///
    /// Written once, by the engine, as soon as the sink is open — which is
    /// before the stream is started and so before any callback runs.
    rate: AtomicU32,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            gain: AtomicU32::new(1.0_f32.to_bits()),
            peak_left: AtomicU32::new(0),
            peak_right: AtomicU32::new(0),
            rate: AtomicU32::new(0),
        }
    }
}

impl Master {
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }

    /// Sets the level, 0 to 1. Anything outside is clamped rather than refused:
    /// a knob dragged past its end is a knob at its end.
    pub fn set_gain(&self, gain: f32) {
        let safe = if gain.is_finite() { gain.clamp(0.0, 1.0) } else { 1.0 };
        self.gain.store(safe.to_bits(), Ordering::Relaxed);
    }

    /// The loudest sample of the last callback, per channel.
    ///
    /// Held rather than replaced: the callback runs every 11 ms and the
    /// interface reads whenever it likes, so a transient between two reads
    /// would be missed if each callback simply overwrote the last.
    fn report(&self, left: f32, right: f32) {
        Self::hold(&self.peak_left, left);
        Self::hold(&self.peak_right, right);
    }

    fn hold(slot: &AtomicU32, value: f32) {
        let mut current = slot.load(Ordering::Relaxed);
        while value > f32::from_bits(current) {
            match slot.compare_exchange_weak(
                current,
                value.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(seen) => current = seen,
            }
        }
    }

    fn set_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Reads the meters and clears them, so the next read is the next span.
    ///
    /// Peaks, not an average: an average of eleven milliseconds is a meter
    /// that never moves.
    pub fn peaks(&self) -> (f32, f32) {
        (
            f32::from_bits(self.peak_left.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.peak_right.swap(0, Ordering::Relaxed)),
        )
    }
}

pub struct Engine {
    decks: [deck::DeckHandle; 2],
    sink: Arc<dyn Sink>,
    sample_rate: u32,
    master: Arc<Master>,
}

impl Engine {
    /// Opens the default output device and starts both decks' threads.
    pub fn new(events: &EventSink) -> Result<Self> {
        Self::with_sink(|render| Ok(Arc::new(CpalSink::open(render)?) as Arc<dyn Sink>), events)
    }

    /// The same engine on a sink of the caller's choosing, which is how it is
    /// tested without an audio device.
    pub fn with_sink<F>(open: F, events: &EventSink) -> Result<Self>
    where
        F: FnOnce(Render) -> Result<Arc<dyn Sink>>,
    {
        let clocks: [Arc<DeckClock>; 2] =
            [Arc::new(DeckClock::default()), Arc::new(DeckClock::default())];

        let mut producers = Vec::with_capacity(2);
        let mut readers = Vec::with_capacity(2);
        for clock in &clocks {
            let (producer, consumer) = rtrb::RingBuffer::<Block>::new(RING_BLOCKS);
            producers.push(producer);
            readers.push(DeckReader {
                consumer,
                clock: Arc::clone(clock),
                current: None,
                offset: 0,
                ramp: Ramp::silent(),
                last: (0.0, 0.0),
                generation: 0,
            });
        }

        // The callback owns the readers outright: nothing else touches them,
        // so it never has to take a lock to read one.
        // The master level and what came out of it. Atomics rather than a
        // lock: the callback is realtime and must never wait for the interface
        // to finish reading a meter.
        let master = Arc::new(Master::default());
        let mixing = Arc::clone(&master);
        // The level a callback is given is one number for the whole buffer, so
        // a hand on the fader arrives as a staircase eleven milliseconds wide.
        // Smoothed per frame instead: see `smooth`. Built at the first
        // callback, because the device's rate is not known until the sink is
        // open and the sink is opened with this closure.
        let mut level: Option<Smoothed> = None;
        let render: Render = Box::new(move |out: &mut [f32]| {
            for reader in &mut readers {
                reader.mix_into(out);
            }
            let target = mixing.gain();
            let level = level.get_or_insert_with(|| Smoothed::new(target, mixing.rate()));
            // Two decks at full level sum past 1.0. A clamp is not a limiter —
            // that is the mixer's job, in P3 — but it keeps a hot sum from
            // reaching the device as a wrap.
            let (mut left, mut right) = (0.0_f32, 0.0_f32);
            for frame in out.chunks_exact_mut(2) {
                let gain = level.step(target);
                for (channel, sample) in frame.iter_mut().enumerate() {
                    let value = (*sample * gain).clamp(-1.0, 1.0);
                    *sample = value;
                    // The meter reads what the device is given, after the
                    // level: a meter before the fader tells you about the file
                    // rather than about what anyone can hear.
                    if channel == 0 {
                        left = left.max(value.abs());
                    } else {
                        right = right.max(value.abs());
                    }
                }
            }
            mixing.report(left, right);
        });

        let sink = open(render)?;
        let sample_rate = sink.sample_rate();
        // Before the stream is started, so the first callback already knows it.
        master.set_rate(sample_rate);

        let mut handles = Vec::with_capacity(2);
        for (deck, (clock, producer)) in Deck::ALL.into_iter().zip(clocks.iter().zip(producers)) {
            clock.set_sample_rate(sample_rate);
            handles.push(deck::spawn(
                deck,
                Arc::clone(clock),
                producer,
                sample_rate,
                Arc::clone(events),
            )?);
        }

        let decks: [deck::DeckHandle; 2] = handles
            .try_into()
            .map_err(|_| DeckError::Device("could not start both decks".to_owned()))?;
        Ok(Self { decks, sink, sample_rate, master })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The master level and its meters.
    pub fn master(&self) -> &Arc<Master> {
        &self.master
    }

    fn deck(&self, deck: Deck) -> Option<&deck::DeckHandle> {
        self.decks.get(deck.index())
    }

    /// Points a deck at a file. Readiness arrives as `DeckEvent::Loaded`,
    /// because opening one means reading from a disk that may be asleep.
    pub fn load(&self, deck: Deck, path: &Path) {
        if let Some(handle) = self.deck(deck) {
            handle.send(deck::Command::Load(PathBuf::from(path)));
        }
        self.settle_device();
    }

    pub fn unload(&self, deck: Deck) {
        if let Some(handle) = self.deck(deck) {
            handle.send(deck::Command::Unload);
        }
        self.settle_device();
    }

    pub fn play(&self, deck: Deck) {
        let Some(handle) = self.deck(deck) else { return };
        if !handle.clock().loaded() {
            return;
        }
        // Playing to the end and pressing play again starts it over, which is
        // what a deck does rather than sitting silently at the end.
        if handle.clock().end_of_stream() && handle.clock().position() >= handle.clock().total() {
            handle.send(deck::Command::Seek(0));
        }
        handle.clock().set_playing(true);
        handle.send(deck::Command::Wake);
        self.settle_device();
    }

    pub fn pause(&self, deck: Deck) {
        let Some(handle) = self.deck(deck) else { return };
        handle.clock().set_playing(false);
        handle.send(deck::Command::Wake);
        self.settle_device();
    }

    /// Moves the playhead, in frames at the device rate.
    pub fn seek_frames(&self, deck: Deck, frame: u64) {
        let Some(handle) = self.deck(deck) else { return };
        // The clock moves now rather than when the first block after the seek
        // is played: a seek while paused must show where it landed.
        handle.clock().set_position(frame.min(handle.clock().total().max(frame)));
        handle.send(deck::Command::Seek(frame));
        handle.send(deck::Command::Wake);
    }

    /// Moves the playhead, in milliseconds.
    pub fn seek_ms(&self, deck: Deck, ms: u64) {
        let frames = ms.saturating_mul(u64::from(self.sample_rate)) / 1000;
        self.seek_frames(deck, frames);
    }

    pub fn snapshot(&self) -> Snapshot {
        let a = self.decks.first().map(|d| d.clock().snapshot()).unwrap_or_default_snapshot();
        let b = self.decks.get(1).map(|d| d.clock().snapshot()).unwrap_or_default_snapshot();
        Snapshot { a, b, sample_rate: self.sample_rate }
    }

    pub fn any_playing(&self) -> bool {
        self.decks.iter().any(|deck| deck.clock().playing())
    }

    /// Whether anything needs the device: playing, or being dragged.
    fn any_sounding(&self) -> bool {
        self.decks.iter().any(|deck| deck.clock().sounding())
    }

    /// Starts a drag on a deck. Audio follows the pointer until `scrub_end`.
    pub fn scrub_begin(&self, deck: Deck) {
        let Some(handle) = self.deck(deck) else { return };
        if !handle.clock().loaded() {
            return;
        }
        handle.clock().set_scrubbing(true);
        handle.send(deck::Command::ScrubBegin);
        self.settle_device();
    }

    /// Where the pointer is now, in milliseconds.
    pub fn scrub_to_ms(&self, deck: Deck, ms: u64) {
        let Some(handle) = self.deck(deck) else { return };
        let frames = ms.saturating_mul(u64::from(self.sample_rate)) / 1000;
        handle.send(deck::Command::ScrubTo(frames));
    }

    /// Ends a drag. The playhead lands under the pointer, not on the read
    /// head, which the audible rate cap leaves behind on a fast drag.
    pub fn scrub_end(&self, deck: Deck) {
        let Some(handle) = self.deck(deck) else { return };
        handle.clock().set_scrubbing(false);
        handle.send(deck::Command::ScrubEnd);
        handle.send(deck::Command::Wake);
        self.settle_device();
    }

    /// Starts the device when a deck needs it and stops it when neither does.
    ///
    /// A stream that is left running costs a callback every 11 ms for silence,
    /// which is exactly the sort of idle cost the budget rules out.
    fn settle_device(&self) {
        let wanted = self.any_sounding();
        let result = if wanted { self.sink.start() } else { self.sink.stop() };
        if let Err(e) = result {
            tracing::error!(error = %e, "the audio device would not change state");
        }
    }
}

/// `Option<DeckSnapshot>` with the empty case spelled out, so a missing deck
/// reads as a stopped one rather than unwrapping.
trait OrEmptySnapshot {
    fn unwrap_or_default_snapshot(self) -> DeckSnapshot;
}

impl OrEmptySnapshot for Option<DeckSnapshot> {
    fn unwrap_or_default_snapshot(self) -> DeckSnapshot {
        self.unwrap_or(DeckSnapshot {
            position_frames: 0,
            total_frames: 0,
            generation: 0,
            sample_rate: 0,
            playing: false,
            loaded: false,
        })
    }
}

/// One deck's side of the audio callback.
///
/// Realtime: no allocation, no lock, no syscall, no `unwrap`. Where anything
/// is missing it writes nothing and leaves silence behind.
struct DeckReader {
    consumer: Consumer<Block>,
    clock: Arc<DeckClock>,
    /// The block being played, and how far into it.
    current: Option<Block>,
    offset: usize,
    /// The envelope every frame this deck contributes goes through.
    ///
    /// Sound is only ever started or cut where this reads zero, which is what
    /// makes play, pause, cue, a beat jump and a seek silent at the join
    /// rather than a click. See `fade`.
    ramp: Ramp,
    /// The last frame actually played, held so that running dry can be faded
    /// rather than cut. See the underrun in `mix_into`.
    last: (f32, f32),
    /// The generation being rendered, which trails the clock's across a seek.
    ///
    /// The clock moves the moment a seek is asked for. The reader keeps
    /// playing what it was playing until the ramp has taken it to zero, and
    /// only then takes the new one up: without that the jump lands in the
    /// middle of a waveform, at whatever height the old one was cut at.
    generation: u32,
}

impl DeckReader {
    fn mix_into(&mut self, out: &mut [f32]) {
        let target = self.clock.generation();
        let sounding = self.clock.sounding();
        // Silent and asked for nothing. Whatever a seek left behind is dropped
        // here rather than handed back when the deck starts again.
        if !sounding && self.ramp.silent_now() {
            self.adopt(target);
            return;
        }

        let frames = out.len() / 2;
        let mut position = self.clock.position();
        let mut done = 0;
        while done < frames {
            // Faded out and the clock has moved on: this is where a seek, a
            // cue or a beat jump actually takes effect, with nothing sounding
            // across the join.
            if self.generation != target && self.ramp.silent_now() {
                self.adopt(target);
            }
            let changing = self.generation != target;
            // Down for a stop, for a seek that has not landed yet, and for the
            // last frames of a track; up for anything else.
            let open = sounding && !changing && !self.ending();
            // Mid-changeover the ring holds where the deck is *going*, so only
            // what is already in hand may be faded out; taking a new block
            // would fade out audio from the seek target and lose it.
            let Some((left, right, at)) = self.next_frame(!changing) else {
                // Nothing to play: the decode thread has not kept up, or the
                // file has ended somewhere the fade did not see coming. The
                // last frame is held and faded down rather than cut — two
                // milliseconds of a held sample decaying is inaudible, and the
                // step it replaces is not. The playhead does not move for it:
                // no audio from the track was played.
                if self.ramp.silent_now() {
                    if changing {
                        self.adopt(target);
                        continue;
                    }
                    break;
                }
                let gain = self.ramp.step(false);
                if let Some(slot) = out.get_mut(done * 2) {
                    *slot += self.last.0 * gain;
                }
                if let Some(slot) = out.get_mut(done * 2 + 1) {
                    *slot += self.last.1 * gain;
                }
                done += 1;
                continue;
            };
            self.last = (left, right);
            let gain = self.ramp.step(open);
            if let Some(slot) = out.get_mut(done * 2) {
                *slot += left * gain;
            }
            if let Some(slot) = out.get_mut(done * 2 + 1) {
                *slot += right * gain;
            }
            position = at;
            done += 1;
            // A stop that has finished fading stops consuming: the frames past
            // it belong to wherever the deck is resumed from.
            if !sounding && self.ramp.silent_now() {
                break;
            }
        }

        self.clock.set_position(position);

        // Ran dry with nothing more coming: the track has ended. An underrun
        // that is not the end leaves the deck playing and writes silence,
        // which is a glitch rather than a stop.
        if done < frames
            && self.current.is_none()
            && self.consumer.is_empty()
            && self.clock.end_of_stream()
        {
            self.clock.set_playing(false);
        }
    }

    /// Takes `generation` as the one being rendered, dropping what is older.
    ///
    /// Only ever called with the ramp at zero, which is what makes the change
    /// silent: the audio before the seek has already been faded out and the
    /// audio after it is faded in from nothing.
    fn adopt(&mut self, generation: u32) {
        self.generation = generation;
        if self.current.as_ref().is_some_and(|block| block.generation < generation) {
            self.current = None;
            self.offset = 0;
        }
    }

    /// The next frame of this deck's audio, and where it leaves the playhead.
    ///
    /// `pop` says whether the ring may be drawn on. Blocks decoded before the
    /// last seek are dropped rather than played: that is what makes a seek
    /// exact.
    fn next_frame(&mut self, pop: bool) -> Option<(f32, f32, u64)> {
        loop {
            if let Some(block) = self.current.as_ref() {
                if self.offset < block.frames as usize {
                    let i = self.offset * 2;
                    let filled = block.filled();
                    let left = filled.get(i).copied().unwrap_or(0.0);
                    let right = filled.get(i + 1).copied().unwrap_or(0.0);
                    self.offset += 1;
                    return Some((left, right, block.position + self.offset as u64));
                }
                self.current = None;
                self.offset = 0;
            }
            if !pop {
                return None;
            }
            match self.consumer.pop() {
                Ok(block) if block.generation >= self.generation => {
                    self.current = Some(block);
                    self.offset = 0;
                }
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// Whether what is left in hand is the last of the track, and short enough
    /// that the fade has to start now to reach zero by the end of it.
    ///
    /// The end of a file is a cut like any other: the last sample is wherever
    /// the music was, and the silence after it is a step down from there.
    fn ending(&self) -> bool {
        if !self.clock.end_of_stream() || !self.consumer.is_empty() {
            return false;
        }
        let left = self
            .current
            .as_ref()
            .map_or(0, |block| (block.frames as usize).saturating_sub(self.offset));
        left <= usize::from(FADE_FRAMES)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, reason = "exact values are set and read back")]
mod master_tests {
    use super::*;

    #[test]
    fn a_new_master_is_open_and_silent() {
        let master = Master::default();
        assert_eq!(master.gain(), 1.0);
        assert_eq!(master.peaks(), (0.0, 0.0));
    }

    #[test]
    fn a_knob_dragged_past_its_end_is_a_knob_at_its_end() {
        let master = Master::default();
        master.set_gain(2.5);
        assert_eq!(master.gain(), 1.0);
        master.set_gain(-1.0);
        assert_eq!(master.gain(), 0.0);
        master.set_gain(f32::NAN);
        assert_eq!(master.gain(), 1.0, "a NaN level would silence the app");
    }

    #[test]
    fn the_meters_read_what_the_device_was_given() {
        let master = Master::default();
        master.report(0.5, 0.25);
        assert_eq!(master.peaks(), (0.5, 0.25));
    }

    #[test]
    fn a_transient_between_two_reads_is_not_lost() {
        // The callback runs every 11 ms and the meter is read when it is read.
        // Overwriting each time would drop the loud callback in between.
        let master = Master::default();
        master.report(0.2, 0.2);
        master.report(0.9, 0.8);
        master.report(0.1, 0.1);
        assert_eq!(master.peaks(), (0.9, 0.8));
    }

    #[test]
    fn reading_the_meters_clears_them_for_the_next_span() {
        let master = Master::default();
        master.report(0.7, 0.7);
        assert_eq!(master.peaks(), (0.7, 0.7));
        assert_eq!(master.peaks(), (0.0, 0.0), "silence since the last read");
    }
}
