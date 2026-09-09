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
mod scrub;
mod sink;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rtrb::Consumer;

pub use clock::{DeckClock, DeckSnapshot};
pub use sink::{CpalSink, NullSink, Render, Sink};

use block::{Block, RING_BLOCKS};

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

pub struct Engine {
    decks: [deck::DeckHandle; 2],
    sink: Arc<dyn Sink>,
    sample_rate: u32,
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
            readers.push(DeckReader { consumer, clock: Arc::clone(clock), current: None, offset: 0 });
        }

        // The callback owns the readers outright: nothing else touches them,
        // so it never has to take a lock to read one.
        let render: Render = Box::new(move |out: &mut [f32]| {
            for reader in &mut readers {
                reader.mix_into(out);
            }
            // Two decks at full level sum past 1.0. A clamp is not a limiter —
            // that is the mixer's job, in P3 — but it keeps a hot sum from
            // reaching the device as a wrap.
            for sample in out.iter_mut() {
                *sample = sample.clamp(-1.0, 1.0);
            }
        });

        let sink = open(render)?;
        let sample_rate = sink.sample_rate();

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
        Ok(Self { decks, sink, sample_rate })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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

    /// Ends a drag. The playhead stays where the head came to rest.
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
}

impl DeckReader {
    fn mix_into(&mut self, out: &mut [f32]) {
        let generation = self.clock.generation();
        if !self.clock.sounding() {
            self.discard_stale(generation);
            return;
        }

        let frames = out.len() / 2;
        let mut done = 0;
        let mut position = self.clock.position();

        while done < frames {
            if !self.holds_playable(generation) {
                self.current = None;
                self.offset = 0;
                // Blocks from before the last seek are dropped rather than
                // played: that is what makes the seek exact.
                while let Ok(block) = self.consumer.pop() {
                    if block.generation >= generation {
                        self.current = Some(block);
                        break;
                    }
                }
            }
            let Some(block) = self.current.as_ref() else { break };
            let available = (block.frames as usize).saturating_sub(self.offset);
            if available == 0 {
                self.current = None;
                self.offset = 0;
                continue;
            }
            let take = available.min(frames - done);
            let from = block.filled().get(self.offset * 2..(self.offset + take) * 2);
            if let (Some(from), Some(into)) = (from, out.get_mut(done * 2..(done + take) * 2)) {
                for (sample, add) in into.iter_mut().zip(from.iter()) {
                    *sample += *add;
                }
            }
            position = block.position + (self.offset + take) as u64;
            self.offset += take;
            done += take;
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

    fn holds_playable(&self, generation: u32) -> bool {
        self.current.as_ref().is_some_and(|block| {
            block.generation >= generation && self.offset < block.frames as usize
        })
    }

    /// Drops what a paused deck is holding once it has been seeked away from,
    /// so the ring does not hand back stale audio when it starts again.
    fn discard_stale(&mut self, generation: u32) {
        if self.current.as_ref().is_some_and(|block| block.generation < generation) {
            self.current = None;
            self.offset = 0;
        }
    }
}
