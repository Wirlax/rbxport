//! One deck: the thread that decodes for it, and the handle that steers it.
//!
//! All blocking work — opening a file, demuxing, decoding, resampling — is on
//! this thread. The audio callback only ever reads blocks that were finished
//! and checked here.
//!
//! While a deck is paused and its ring is full there is nothing that can
//! change except a command, so the thread blocks on the channel rather than
//! polling. That is what keeps an idle app at no measurable CPU.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use rtrb::Producer;

use crate::block::{Block, BLOCK_FRAMES, RING_BLOCKS};
use crate::clock::DeckClock;
use crate::decode::Streamer;
use crate::{Deck, DeckEvent, EventSink};

/// What the control side asks a deck to do.
pub enum Command {
    Load(PathBuf),
    /// Both play and pause are already in the clock when this arrives; the
    /// command exists to wake the thread from its blocking wait.
    Wake,
    Seek(u64),
    Unload,
    Quit,
}

/// How long the thread waits between top-ups while the ring is full and audio
/// is running. Short enough that a device buffer can never outrun it.
const TOP_UP_WAIT: Duration = Duration::from_millis(2);

pub struct DeckHandle {
    commands: Sender<Command>,
    clock: Arc<DeckClock>,
}

impl DeckHandle {
    pub fn clock(&self) -> &Arc<DeckClock> {
        &self.clock
    }

    pub fn send(&self, command: Command) {
        // A deck whose thread has gone is a deck that cannot be steered; the
        // clock stops moving, which the interface already draws as stopped.
        if self.commands.send(command).is_err() {
            tracing::error!("a deck's decode thread has stopped");
        }
    }
}

impl Drop for DeckHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }
}

/// Starts a deck's decode thread and returns the handle to it.
pub fn spawn(
    deck: Deck,
    clock: Arc<DeckClock>,
    producer: Producer<Block>,
    device_rate: u32,
    events: EventSink,
) -> std::io::Result<DeckHandle> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_clock = Arc::clone(&clock);
    std::thread::Builder::new()
        .name(format!("rbl-deck-{}", deck.name()))
        .spawn(move || {
            let mut worker = Worker {
                deck,
                clock: worker_clock,
                producer,
                device_rate,
                events,
                streamer: None,
                generation: 0,
            };
            worker.run(&rx);
        })?;
    Ok(DeckHandle { commands: tx, clock })
}

struct Worker {
    deck: Deck,
    clock: Arc<DeckClock>,
    producer: Producer<Block>,
    device_rate: u32,
    events: EventSink,
    streamer: Option<Streamer>,
    generation: u32,
}

impl Worker {
    fn run(&mut self, commands: &Receiver<Command>) {
        loop {
            // A ring's worth at most, then look at the channel again.
            //
            // Unbounded, this starves its own commands: a consumer draining as
            // fast as this fills — anything faster than realtime — means the
            // ring is never full, `produce` never returns false, and a seek
            // sits in the channel while the thread decodes the rest of the
            // track. Seeking four minutes into a track took two seconds
            // because of it.
            let mut produced = false;
            for _ in 0..RING_BLOCKS {
                if !self.produce() {
                    break;
                }
                produced = true;
            }

            let waiting = if produced || self.clock.playing() {
                // Playing: top up as the callback drains, without spinning.
                match commands.recv_timeout(TOP_UP_WAIT) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                // Paused with nothing to decode: nothing but a command can
                // change that, so block rather than poll.
                match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            };

            if let Some(command) = waiting {
                if !self.handle(command) {
                    return;
                }
            }
        }
    }

    /// Returns false when the deck has been told to quit.
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Load(path) => self.load(&path),
            Command::Seek(frame) => self.seek(frame),
            Command::Unload => self.unload(),
            Command::Wake => {}
            Command::Quit => return false,
        }
        true
    }

    fn load(&mut self, path: &std::path::Path) {
        self.clock.set_playing(false);
        self.streamer = None;
        self.clock.set_loaded(false);
        self.clock.set_end_of_stream(false);

        match Streamer::open(path, self.device_rate) {
            Ok(streamer) => {
                let total = streamer.total_frames();
                self.generation = self.clock.bump_generation();
                self.clock.set_position(0);
                self.clock.set_total(total);
                self.clock.set_sample_rate(self.device_rate);
                self.streamer = Some(streamer);
                self.clock.set_loaded(true);
                (self.events)(DeckEvent::Loaded {
                    deck: self.deck,
                    total_frames: total,
                    sample_rate: self.device_rate,
                });
            }
            Err(e) => {
                self.clock.set_total(0);
                self.clock.set_position(0);
                (self.events)(DeckEvent::Error { deck: self.deck, message: e.to_string() });
            }
        }
    }

    fn seek(&mut self, frame: u64) {
        let Some(streamer) = self.streamer.as_mut() else { return };
        match streamer.seek(frame) {
            Ok(landed) => {
                self.clock.set_end_of_stream(false);
                // The position first, then the generation: the callback reads
                // them in that order, and must never take a new generation's
                // blocks while still believing the old position.
                self.clock.set_position(landed);
                self.generation = self.clock.bump_generation();
            }
            Err(e) => {
                (self.events)(DeckEvent::Error { deck: self.deck, message: e.to_string() });
            }
        }
    }

    fn unload(&mut self) {
        self.clock.set_playing(false);
        self.streamer = None;
        self.generation = self.clock.bump_generation();
        self.clock.set_loaded(false);
        self.clock.set_position(0);
        self.clock.set_total(0);
        self.clock.set_end_of_stream(false);
    }

    /// Decodes one block into the ring. False when there is nothing to add.
    fn produce(&mut self) -> bool {
        if self.producer.is_full() {
            return false;
        }
        let generation = self.generation;
        let Some(streamer) = self.streamer.as_mut() else { return false };
        if streamer.finished() {
            self.clock.set_end_of_stream(true);
            return false;
        }

        let mut block = Block::empty(generation, streamer.position());
        let frames = match streamer.fill(&mut block.samples) {
            Ok(frames) => frames,
            Err(e) => {
                // A decode that fails mid-track stops the deck rather than
                // playing whatever was left in the buffer.
                self.clock.set_end_of_stream(true);
                (self.events)(DeckEvent::Error { deck: self.deck, message: e.to_string() });
                return false;
            }
        };
        if frames == 0 {
            if streamer.finished() {
                self.clock.set_end_of_stream(true);
            }
            return false;
        }
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        self.producer.push(block).is_ok()
    }
}
