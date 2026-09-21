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
use std::time::{Duration, Instant};

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
    /// Where the pointer is, in device-rate frames, and when it said so.
    ///
    /// The instant is stamped where the report enters the engine rather than
    /// counted in blocks on the way out. Blocks are 11.6 ms and a hand
    /// crossing thirty pixels a second reports every 33 ms, so rounding the
    /// gap between reports to whole blocks quantises the measured speed by a
    /// sixth — the same mistake on the time axis that rounding the position to
    /// whole milliseconds was on the distance axis. See `Engine::scrub_to_ms`.
    ScrubTo(u64, Instant),
    /// The drag is over: the streamer picks up where the head was left.
    ScrubEnd,
    /// How fast to play, as a multiple of the file's own speed.
    SetTempo(f32),
    /// Master Tempo: whether the pitch is held while the speed changes.
    SetMasterTempo(bool),
    /// The key, in semitones from the track's own.
    SetKeyShift(i8),
    Unload,
    Quit,
}

/// How far the key can be shifted either way, in semitones: an octave.
pub const KEY_SHIFT_RANGE: i8 = 12;

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
                last_report: None,
                window: PcmWindow::empty(),
                keylock: key_lock(device_rate),
                varispeed: Varispeed::new(device_rate),
                tempo: 1.0,
                master_tempo: false,
                key_shift: 0,
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
    /// When the pointer last reported, so the next one can be timed.
    last_report: Option<Instant>,
    /// Decoded audio around that head. Emptied when the drag ends, because
    /// several megabytes for a gesture that is over is several megabytes
    /// nobody asked for.
    window: PcmWindow,
    /// The two ways to play at another speed: with the pitch held, and with
    /// the pitch moving as a record's does. Both are built at load, because
    /// building one takes an allocation and the switch between them is a
    /// button somebody presses mid-track.
    ///
    /// Boxed because which one holds the pitch is a build-time decision — see
    /// [`key_lock`] — and the hot path already went through `&mut dyn
    /// Stretcher`, so this costs one allocation at load and nothing per block.
    keylock: Box<dyn Stretcher>,
    varispeed: Varispeed,
    tempo: f32,
    master_tempo: bool,
    /// Semitones from the track's own key. Any shift puts the key-lock
    /// stretcher in the path, since only it can move the pitch on its own;
    /// with Master Tempo off it is asked for the pitch a record would have
    /// at this speed, and the shift on top of that.
    key_shift: i8,
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
            Command::ScrubTo(frame, at) => self.scrub_to(frame, at),
            Command::ScrubEnd => self.scrub_end(),
            Command::SetTempo(tempo) => self.set_tempo(tempo),
            Command::SetMasterTempo(on) => {
                // The two hold different audio, so switching between them
                // starts the new one from where the old one had reached
                // rather than from what it happened to have buffered.
                let was = self.keylock_in_path();
                self.master_tempo = on;
                self.apply_pitch();
                if was != self.keylock_in_path() {
                    self.restart_stretch();
                }
            }
            Command::SetKeyShift(semitones) => {
                let was = self.keylock_in_path();
                self.key_shift = semitones.clamp(-KEY_SHIFT_RANGE, KEY_SHIFT_RANGE);
                self.clock.set_key_shift(self.key_shift);
                self.apply_pitch();
                if was != self.keylock_in_path() {
                    self.restart_stretch();
                }
            }
            Command::Unload => self.unload(),
            Command::Wake => {}
            Command::Quit => return false,
        }
        true
    }

    fn load(&mut self, path: &std::path::Path) {
        self.clock.set_start_in(0);
        self.clock.set_loop(None);
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

    /// The loop the deck is inside, if it is inside one.
    fn active_loop(&self) -> Option<(u64, u64)> {
        if self.clock.looping() { self.clock.loop_range() } else { None }
    }

    /// Back to the loop's in point, quietly: no generation change, so the
    /// callback plays straight on from the block before the jump to the
    /// block after it with nothing faded, which is what makes a loop seam
    /// inaudible. The blocks already in the ring were trimmed to the out
    /// point by `produce`, so nothing past it is queued.
    fn loop_jump(&mut self, frame: u64) {
        let Some(streamer) = self.streamer.as_mut() else { return };
        match streamer.seek(frame) {
            Ok(landed) => {
                self.clock.set_end_of_stream(false);
                self.head = landed as f64;
                self.restart_stretch();
            }
            Err(e) => {
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
                // Adopted, not bumped: `seek_frames` already moved the counter
                // when it moved the position, so that the callback stopped
                // trusting the ring at the same instant. Bumping again here
                // would start a second changeover for one seek.
                self.generation = self.clock.generation();
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
        self.apply_pitch();
    }

    /// Whether the key-lock stretcher, rather than plain resampling, is
    /// playing: Master Tempo on, or a key shift that only it can make.
    fn keylock_in_path(&self) -> bool {
        self.master_tempo || self.key_shift != 0
    }

    /// The pitch the key-lock stretcher is asked for: the shift, on top of
    /// the pitch a record would have at this speed when Master Tempo is off.
    fn apply_pitch(&mut self) {
        let shift = 2_f32.powf(f32::from(self.key_shift) / 12.0);
        let base = if self.master_tempo { 1.0 } else { self.tempo };
        self.keylock.set_pitch_scale(base * shift);
    }

    /// Whichever of the two is in the path.
    fn stretcher(&mut self) -> &mut dyn Stretcher {
        if self.keylock_in_path() { &mut *self.keylock } else { &mut self.varispeed }
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
        self.clock.set_start_in(0);
        self.clock.set_loop(None);
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
        self.scrubber = Some(Scrubber::new(at, self.device_rate));
        // A new drag times its reports from scratch; the gap since the last
        // one is however long ago the previous drag was, which is not a speed.
        self.last_report = None;
        // The blocks already in flight belong to normal playback and are at the
        // wrong place and the wrong speed; a new generation drops them.
        self.generation = self.clock.bump_generation();
        self.fill_window(at);
        self.clock.set_scrubbing(true);
    }

    fn scrub_to(&mut self, frame: u64, at: Instant) {
        // Output frames since the report before this one, which is what the
        // head's speed is measured over. Nothing for the first report of a
        // drag: there is no report before it to measure against.
        let since = self.last_report.replace(at).map_or(0.0, |last| {
            at.saturating_duration_since(last).as_secs_f64() * f64::from(self.device_rate)
        });
        if let Some(scrubber) = self.scrubber.as_mut() {
            scrubber.aim(frame, since);
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
        // A short fill is the end of the track; a start of 0 is its top. On
        // either the window can go no further, and the head is not asked to
        // keep a margin from an edge that is the track's own.
        let at_end = filled < wanted;
        self.window = PcmWindow { start, samples, at_start: start == 0, at_end };
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
        // At or past the out point: round again before another frame is
        // decoded. A loop set behind the head, or a seek past its end while
        // it is on, comes back here too.
        let active = self.active_loop();
        if let Some((from, to)) = active {
            let at = if self.stretching() { self.head as u64 } else { self.streamer.as_ref().map_or(0, Streamer::position) };
            if at >= to {
                self.loop_jump(from);
            }
        }
        let stretching = self.stretching();
        let Some(streamer) = self.streamer.as_mut() else { return false };
        if streamer.finished() {
            self.clock.set_end_of_stream(true);
            return false;
        }

        // Through the stretcher whenever the speed or the key is not the
        // file's own; at unity with no shift the decoded audio goes straight.
        if stretching {
            return self.produce_stretched(generation);
        }
        let mut block = Block::empty(generation, 0);
        // Only up to the out point, so the jump lands on the frame: a block
        // that ran past it would play the audio beyond the loop first.
        let room = active.map_or(BLOCK_FRAMES, |(_, to)| {
            usize::try_from(to.saturating_sub(streamer.position())).unwrap_or(BLOCK_FRAMES).clamp(1, BLOCK_FRAMES)
        });
        let Some(target) = block.samples.get_mut(..room * 2) else { return false };
        let frames = match streamer.fill(target) {
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
        // Stamped after the fill, not before: the first fill after a seek
        // starts by discarding what the demuxer overshot by, so the position
        // beforehand is the packet boundary it landed on, not the frame the
        // block's audio begins at. Stamping that put the playhead a few
        // hundred frames early for one block after every seek.
        block.position = streamer.position() - frames as u64;
        self.head = streamer.position() as f64;
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        let pushed = self.producer.push(block).is_ok();
        // Reached the out point with this block: round now, so the next block
        // decoded is the in point rather than a bufferful past the out.
        if let Some((from, to)) = active {
            if self.streamer.as_ref().is_some_and(|s| s.position() >= to) {
                self.loop_jump(from);
            }
        }
        pushed
    }

    /// Whether the audio goes through a stretcher rather than straight: the
    /// speed or the key is not the file's own.
    fn stretching(&self) -> bool {
        (self.tempo - 1.0).abs() > f32::EPSILON || self.key_shift != 0
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
        let mut block = Block::empty(generation, self.head as u64);
        let mut frames = 0;

        // Feed, take what came of it, feed again — until the block is full or
        // there is nothing left to feed it with. One pass is not enough for
        // every backend: Rubber Band takes the block it asked for, hands back
        // what that block made, and wants nothing more until it has been
        // drained, so a single feed-then-pull filled exactly half of every
        // block. WSOLA fills one in a pass and leaves this loop after it.
        while frames < BLOCK_FRAMES {
            if !self.top_up_stretcher() {
                break;
            }
            let Some(rest) = block.samples.get_mut(frames * 2..) else { break };
            let got = self.stretcher().pull(rest);
            if got == 0 {
                break;
            }
            frames += got;
        }

        if frames == 0 {
            return false;
        }
        self.head += frames as f64 * tempo;
        block.frames = u16::try_from(frames.min(BLOCK_FRAMES)).unwrap_or(0);
        let pushed = self.producer.push(block).is_ok();
        // Stretched, the head is counted rather than read, so the loop
        // rounds at a block's granularity — a few milliseconds at most —
        // rather than on the frame [ASSUME: close enough for a beat loop;
        // sample-exact would mean trimming the stretcher's output].
        if let Some((from, to)) = self.active_loop() {
            if self.head as u64 >= to {
                self.loop_jump(from);
            }
        }
        pushed
    }

    /// Gives the stretcher everything it asks for that there is input for.
    ///
    /// False when the stream is over and there is nothing more to give, which
    /// is what stops the loop above rather than a short pull.
    fn top_up_stretcher(&mut self) -> bool {
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
                if self.master_tempo || self.key_shift != 0 { &mut *self.keylock } else { &mut self.varispeed };
            let Some(from) = self.feed.get(..take * 2) else { break };
            let taken = stretcher.feed(from);
            // What was not taken stays at the front for the next pass.
            self.feed.copy_within(taken * 2..self.fed * 2, 0);
            self.fed -= taken;
        }
        true
    }
}

/// The stretcher behind MASTER TEMPO.
///
/// Rubber Band R3 where the `rubberband` feature is on, which is the default
/// and the GPL build; the WSOLA backend written for this crate otherwise, and
/// also if Rubber Band will not allocate, because a deck that plays with the
/// pitch drifting is better than a deck that does not play.
fn key_lock(device_rate: u32) -> Box<dyn Stretcher> {
    #[cfg(feature = "rubberband")]
    if let Some(stretcher) = crate::rubberband::RubberBand::new(device_rate) {
        return Box::new(stretcher);
    }
    Box::new(Wsola::new(device_rate))
}
