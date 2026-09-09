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
use crate::scrub::{PcmWindow, Scrubber, WINDOW_REACH};
use crate::stretch::{Stretcher, Varispeed, Wsola};
use crate::{Deck, DeckEvent, EventSink};

/// What the control side asks a deck to do.
pub enum Command {
    Load(PathBuf),
    /// Both play and pause are already in the clock when this arrives; the
    /// command exists to wake the thread from its blocking wait.
    Wake,
    Seek(u64),
    /// A drag has started. The deck decodes a window around the playhead and
    /// starts producing from it at whatever rate the drag asks for.
    ScrubBegin,
    /// Where the pointer is now, in device-rate frames.
    ScrubTo(u64),
    /// The drag is over: the streamer picks up where the head was left.
    ScrubEnd,
    /// How fast to play, as a multiple of the file's own speed.
    SetTempo(f32),
    /// Master Tempo: whether the pitch is held while the speed changes.
    SetMasterTempo(bool),
    Unload,
    Quit,
}

/// How long the thread waits between top-ups while the ring is full and audio
/// is running. Short enough that a device buffer can never outrun it.
const TOP_UP_WAIT: Duration = Duration::from_millis(2);

/// Blocks kept queued while a drag is running: about 35 ms.
///
/// Playback wants the full ring, which absorbs a decode hiccup. A drag wants
/// the opposite — every block already queued is a block of the pointer's past.
const SCRUB_BLOCKS: usize = 3;

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
                scrubber: None,
                window: PcmWindow::empty(),
                keylock: Wsola::new(device_rate),
                varispeed: Varispeed::new(device_rate),
                tempo: 1.0,
                master_tempo: false,
                head: 0.0,
                feed: vec![0.0; BLOCK_FRAMES * 2],
                fed: 0,
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
    /// The read head, present only while a drag is running.
    scrubber: Option<Scrubber>,
    /// Decoded audio around that head. Emptied when the drag ends, because
    /// several megabytes for a gesture that is over is several megabytes
    /// nobody asked for.
    window: PcmWindow,
    /// The two ways to play at another speed: with the pitch held, and with
    /// the pitch moving as a record's does. Both are built at load, because
    /// building one takes an allocation and the switch between them is a
    /// button somebody presses mid-track.
    keylock: Wsola,
    varispeed: Varispeed,
    tempo: f32,
    master_tempo: bool,
    /// Where the audio coming out of the stretcher sits in the track, in
    /// input frames.
    ///
    /// Not the streamer's position: by the time a block comes out of the
    /// stretcher the streamer has read a bufferful past it, and a playhead
    /// that ran 60 ms ahead of the sound would put every cue in the wrong
    /// place. Advanced by what the output consumes instead.
    head: f64,
    /// Frames of decoded audio waiting to go into the stretcher.
    feed: Vec<f32>,
    /// How much of `feed` has been handed over.
    fed: usize,
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

            let waiting = if produced || self.clock.sounding() {
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
            Command::ScrubBegin => self.scrub_begin(),
            Command::ScrubTo(frame) => self.scrub_to(frame),
            Command::ScrubEnd => self.scrub_end(),
            Command::SetTempo(tempo) => self.set_tempo(tempo),
            Command::SetMasterTempo(on) => {
                // The two hold different audio, so switching between them
                // starts the new one from where the old one had reached
                // rather than from what it happened to have buffered.
                self.master_tempo = on;
                self.restart_stretch();
            }
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
                self.head = 0.0;
                self.restart_stretch();
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
                self.head = landed as f64;
                self.restart_stretch();
                self.generation = self.clock.bump_generation();
            }
            Err(e) => {
                (self.events)(DeckEvent::Error { deck: self.deck, message: e.to_string() });
            }
        }
    }

    /// How fast to play, as a multiple of the file's own speed.
    fn set_tempo(&mut self, tempo: f32) {
        let safe = if tempo.is_finite() {
            tempo.clamp(crate::stretch::MIN_RATIO, crate::stretch::MAX_RATIO)
        } else {
            1.0
        };
        self.tempo = safe;
        self.keylock.set_ratio(safe);
        self.varispeed.set_ratio(safe);
        self.clock.set_tempo(safe);
    }

    /// Whichever of the two is in the path.
    fn stretcher(&mut self) -> &mut dyn Stretcher {
        if self.master_tempo { &mut self.keylock } else { &mut self.varispeed }
    }

    /// Empties both, so nothing of the last position or the last mode is
    /// played after a seek or a switch.
    fn restart_stretch(&mut self) {
        self.keylock.reset();
        self.varispeed.reset();
        self.fed = 0;
        self.feed.fill(0.0);
    }

    fn unload(&mut self) {
        self.scrubber = None;
        self.window = PcmWindow::empty();
        self.clock.set_scrubbing(false);
        self.clock.set_playing(false);
        self.streamer = None;
        self.generation = self.clock.bump_generation();
        self.clock.set_loaded(false);
        self.clock.set_position(0);
        self.clock.set_total(0);
        self.clock.set_end_of_stream(false);
    }

    /// Starts a drag: the head begins where the playhead is.
    fn scrub_begin(&mut self) {
        if self.streamer.is_none() {
            return;
        }
        let at = self.clock.position();
        self.scrubber = Some(Scrubber::new(at));
        // The blocks already in flight belong to normal playback and are at the
        // wrong place and the wrong speed; a new generation drops them.
        self.generation = self.clock.bump_generation();
        self.fill_window(at);
        self.clock.set_scrubbing(true);
    }

    fn scrub_to(&mut self, frame: u64) {
        if let Some(scrubber) = self.scrubber.as_mut() {
            scrubber.aim(frame);
        }
    }

    /// Ends a drag, leaving the playhead under the pointer.
    ///
    /// Where the pointer is, not where the read head got to. The head is
    /// capped at `MAX_RATE` so the drag stays audible, which leaves it behind
    /// the hand on a fast one and barely moved at all on a click; landing on
    /// it turned a click on the overview into a jump that sprang back.
    fn scrub_end(&mut self) {
        let Some(scrubber) = self.scrubber.take() else { return };
        self.clock.set_scrubbing(false);
        self.window = PcmWindow::empty();
        // The streamer has been sitting wherever the window was filled from,
        // so it has to be put where the drag finished before playback resumes.
        let total = self.clock.total();
        let at = if total > 0 { scrubber.target().min(total) } else { scrubber.target() };
        self.seek(at);
    }

    /// Decodes the window a drag reads from, centred on `at`.
    ///
    /// One demuxer seek and a few seconds of decoding, which is what buys the
    /// thousands of reads a drag makes without touching the file again.
    fn fill_window(&mut self, at: u64) {
        let Some(streamer) = self.streamer.as_mut() else { return };
        let start = at.saturating_sub(WINDOW_REACH);
        if streamer.seek(start).is_err() {
            self.window = PcmWindow::empty();
            return;
        }
        let wanted = (WINDOW_REACH * 2) as usize;
        let mut samples = vec![0.0_f32; wanted * 2];
        let mut filled = 0_usize;
        while filled < wanted {
            let Some(chunk) = samples.get_mut(filled * 2..) else { break };
            // A short read or a decode error both mean the window is as long
            // as it is going to get; a drag past its end hears silence, which
            // is what the end of a record sounds like.
            match streamer.fill(chunk) {
                Ok(frames) if frames > 0 => filled += frames,
                _ => break,
            }
        }
        samples.truncate(filled * 2);
        self.window = PcmWindow { start, samples };
    }

    /// One block of a drag. False when there is nothing to add.
    fn produce_scrub(&mut self) -> bool {
        // A shallow ring, not the full sixteen blocks. What is already queued
        // has to play out before the drag's next move is heard, and 186 ms of
        // that is a drag that answers the pointer a fifth of a second late and
        // keeps sounding that long after it stops.
        if RING_BLOCKS - self.producer.slots() >= SCRUB_BLOCKS {
            return false;
        }
        let generation = self.generation;
        let Some(scrubber) = self.scrubber.as_mut() else { return false };
        // Nothing until the pointer has said where it is going. Producing
        // at-rest blocks in the meantime fills the ring with silence, and that
        // silence has to play out before the first sound of the drag.
        if !scrubber.started() {
            return false;
        }
        // Refill before the head reaches the edge, not after: a demuxer seek
        // costs more than a block, and running off the end is silence.
        let at = scrubber.cursor();
        let comfortable = self.window.comfortable(at as f64, WINDOW_REACH / 4);
        if !comfortable {
            self.fill_window(at);
        }
        let Some(scrubber) = self.scrubber.as_mut() else { return false };
        let mut block = Block::empty(generation, scrubber.cursor());
        let frames = scrubber.render(&self.window, &mut block.samples);
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        self.producer.push(block).is_ok()
    }

    /// Decodes one block into the ring. False when there is nothing to add.
    fn produce(&mut self) -> bool {
        if self.scrubber.is_some() {
            return self.produce_scrub();
        }
        if self.producer.is_full() {
            return false;
        }
        let generation = self.generation;
        let Some(streamer) = self.streamer.as_mut() else { return false };
        if streamer.finished() {
            self.clock.set_end_of_stream(true);
            return false;
        }

        if (self.tempo - 1.0).abs() > f32::EPSILON {
            return self.produce_stretched(generation);
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
        self.head = streamer.position() as f64 + frames as f64;
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        self.producer.push(block).is_ok()
    }

    /// The same, with the tempo control in the path.
    ///
    /// The block's position is not the streamer's: by the time audio comes out
    /// of a stretcher the streamer has read a bufferful past it, and a
    /// playhead running ahead of its own sound puts every cue in the wrong
    /// place. It is counted forward instead, by what each block of output
    /// consumed at the speed it was played at.
    fn produce_stretched(&mut self, generation: u32) -> bool {
        let tempo = f64::from(self.tempo);
        // Top the stretcher up first: it needs a segment and its search window
        // before it can produce anything at all.
        loop {
            let wanted = self.stretcher().wants();
            if wanted == 0 || self.stretcher().ready(BLOCK_FRAMES) {
                break;
            }
            if self.fed == 0 {
                let Some(streamer) = self.streamer.as_mut() else { return false };
                let frames = match streamer.fill(&mut self.feed) {
                    Ok(frames) => frames,
                    Err(e) => {
                        self.clock.set_end_of_stream(true);
                        (self.events)(DeckEvent::Error {
                            deck: self.deck,
                            message: e.to_string(),
                        });
                        return false;
                    }
                };
                if frames == 0 {
                    if streamer.finished() {
                        self.clock.set_end_of_stream(true);
                    }
                    break;
                }
                self.fed = frames;
            }
            let take = self.fed.min(wanted);
            // The fields taken apart by hand rather than through `stretcher`,
            // so the buffer can be lent to the stretcher without copying it:
            // one method borrowing all of `self` would have meant a fresh
            // allocation for every block a stretched deck plays.
            let stretcher: &mut dyn Stretcher =
                if self.master_tempo { &mut self.keylock } else { &mut self.varispeed };
            let Some(from) = self.feed.get(..take * 2) else { break };
            let taken = stretcher.feed(from);
            // What was not taken stays at the front for the next pass.
            self.feed.copy_within(taken * 2..self.fed * 2, 0);
            self.fed -= taken;
        }

        let mut block = Block::empty(generation, self.head as u64);
        let frames = self.stretcher().pull(&mut block.samples);
        if frames == 0 {
            return false;
        }
        self.head += frames as f64 * tempo;
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        self.producer.push(block).is_ok()
    }
}
