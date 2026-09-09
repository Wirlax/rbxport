//! Dragging the waveform the way a hand moves a record.
//!
//! Seeking on every pointer move gives you the *right place* and the wrong
//! sound: each seek restarts the decoder, so what you hear is a series of
//! clean bursts at normal speed. A record does not do that. It plays whatever
//! is under the needle at whatever speed the hand is moving it, forwards or
//! backwards, and stops making a sound when the hand stops.
//!
//! So: a window of decoded audio around the cursor, and a read head that moves
//! through it at the drag's own rate. Both halves are here and neither knows
//! about threads — the decode thread fills the window, and calls `render` to
//! produce a block.
//!
//! A hand that pauses is a stop like any other, so the same envelope guards it:
//! cutting to silence the moment the head rests is a step from wherever the
//! waveform happened to be, and on bass that is a crack every time the hand
//! holds still. See `fade`.

use crate::fade::Ramp;

/// Frames either side of the cursor the window holds.
///
/// Two seconds at 44.1 kHz, so a drag has four seconds of room before the
/// demuxer is asked for anything, and a refill decodes four seconds rather
/// than eight. That matters: a refill runs on the decode thread, and while it
/// runs no blocks are produced — an eight-second one outran what a drag has
/// buffered and left a hole in the middle of it.
pub const WINDOW_REACH: u64 = 44_100 * 2;

/// The fastest a drag can play, as a multiple of normal speed.
///
/// Past this a flick of the wrist is a burst of noise rather than a sound you
/// can aim with, and the interpolation has nothing useful left to read.
const MAX_RATE: f64 = 8.0;

/// How sharply the rate follows the pointer, per block.
///
/// A pointer emits moves at about the screen's rate and blocks come out faster
/// than that, so some blocks see no new target. Following the error rather
/// than snapping to it is what stops those blocks sounding like a stutter.
///
/// The value is a measurement, not a taste. The distance left to the pointer
/// is a sawtooth — the target jumps once a screen frame, the head eats into
/// the gap over the blocks before the next jump — so whatever survives this
/// filter is heard as the speed rising and falling under a hand that is
/// moving evenly. Swept against a steady drag (`scrub_ripple` below):
///
/// | value | spread of the rate | blocks to reach speed |
/// |-------|--------------------|-----------------------|
/// | 0.40  | 9.0%               | 3                     |
/// | 0.25  | 5.9%               | 4                     |
/// | 0.15  | 3.7%               | 6                     |
/// | 0.10  | 2.5%               | 7                     |
/// | 0.06  | 3.3-5.1%           | 9, and it starts resting mid-drag |
///
/// 0.15 is where the warble stops being the loudest thing about a slow drag
/// without the head taking so long to reach speed that it feels soft. Below
/// about 0.08 the loop is too slow to track the hand at all and the ripple
/// climbs again.
const SMOOTH: f64 = 0.15;

/// Blocks of audio the read head aims to be behind the pointer.
///
/// The ring is ahead of the callback, so a head that closed the whole gap in
/// one block would arrive early and then wait. Two blocks is about 23 ms.
const LOOKAHEAD: f64 = 2.0;

/// Below this the record has stopped, and silence is what a stopped record
/// makes. Interpolating at a rate this low is a held sample, which is a hum.
const REST_RATE: f64 = 0.01;

/// The furthest the head is allowed to fall behind the pointer, in frames.
///
/// About 90 ms — what the head can cover at `MAX_RATE` in a couple of blocks. A
/// flick moves the pointer further than the head can render, and without this
/// the head grinds through the gap at eight times for as long as it takes,
/// still playing seconds after the hand has stopped. Capping the lag makes a
/// fast drag sound like what it is: a burst of the music it passed over, then
/// silence, with the head where the pointer left it.
const MAX_LAG: f64 = 4096.0;

/// Blocks without a pointer report before the head is allowed to settle.
///
/// A pointer emits about one move a screen frame and a block is 11.6 ms, so
/// two or three blocks pass between moves during an ordinary drag. Settling
/// any sooner than that silences a hand that is still moving.
///
/// Added to `STOPPED_INTERVALS` rather than used alone: a mouse only reports
/// once the cursor has crossed a whole pixel, so a slow hand reports far more
/// slowly than a screen frame and a fixed threshold reads it as stopped.
const SETTLE_BLOCKS: f64 = 4.0;

/// How many of the hand's own reporting intervals count as it having stopped.
///
/// A hand crossing ten pixels a second on a waveform zoomed to twelve bars
/// reports every 100 ms, and one crossing five hundred reports every other
/// block. There is no one number of blocks that means "stopped" for both, so
/// the hand is timed against itself: gone quiet for a couple of its own
/// intervals, and it has stopped rather than merely being slow.
const STOPPED_INTERVALS: f64 = 2.0;

/// How sharply the hand's measured speed is followed, per report.
///
/// Reports are what carry the speed, and a slow hand sends few of them, so
/// this cannot be as gentle as `SMOOTH`: filtering hard here is a head that
/// takes half a second to notice the hand has sped up.
///
/// Swept against a pixel-crossing hand (`a_hand_crossing_pixels_slowly_still_
/// turns_the_record`), reading the warble averaged over the speeds it covers
/// and at the worst of them, against how many reports it takes to follow a
/// hand that has changed speed:
///
/// | value | warble, mean | worst | follows a change in |
/// |-------|--------------|-------|---------------------|
/// | 0.20  | 4.3%         | 6.3%  | 11 reports          |
/// | 0.30  | 4.5%         | 6.8%  | 7 reports           |
/// | 0.40  | 4.8%         | 7.1%  | 5 reports           |
/// | 0.60  | 5.2%         | 7.7%  | 4 reports           |
/// | 0.80  | 5.6%         | 8.1%  | 3 reports           |
///
/// The warble is flat across the useful range and the responsiveness is not,
/// so this is chosen for the second: two tenths of a percent of warble is not
/// audible, and four more reports at a hundred milliseconds each is nearly
/// half a second of the head ignoring a hand that has sped up.
const SPEED_SMOOTH: f64 = 0.30;

/// How sharply the measured gap between reports is followed.
///
/// Gentler than the speed: the interval decides when the hand counts as
/// stopped, and a single late report must not be read as the hand slowing.
const INTERVAL_SMOOTH: f64 = 0.2;

/// Blocks a report interval is taken to be before one has been measured.
///
/// A pointer coalesced to the screen's rate against an 11.6 ms block, which is
/// what a hand moving at any speed worth hearing reports at.
const INITIAL_INTERVAL: f64 = 2.0;

/// How hard the head is pulled back onto the pointer, per block.
///
/// The speed alone would drift: a hand's measured speed is never exactly its
/// real one, and the error accumulates into the head sitting further and
/// further behind. This closes that gap — but gently, because the gap is also
/// where the sprint came from. The old loop *was* this term with the strength
/// at one: a pixel of a twelve-bar waveform is 1102 frames, about one `span`,
/// so every pixel the cursor crossed commanded a full-speed sprint however
/// long the hand had taken to cross it.
///
/// It trades the two ways the head can be wrong against each other — how
/// steady it runs against how close to the hand's own speed it settles — and
/// the warble is the audible one:
///
/// | value | warble, mean | worst | speed off by |
/// |-------|--------------|-------|--------------|
/// | 0.05  | 4.3%         | 7.0%  | 2.9%         |
/// | 0.10  | 4.8%         | 7.1%  | 2.5%         |
/// | 0.15  | 5.1%         | 7.2%  | 2.2%         |
/// | 0.25  | 5.4%         | 7.5%  | 1.6%         |
///
/// Being a steady 2.5% off the hand's speed is a pitch nobody hears as wrong;
/// wandering by 5% of itself several times a second is a warble anybody does.
/// Below 0.05 the head takes too long to recover from having been clamped.
const CORRECT: f64 = 0.10;

/// Decoded audio around the cursor: interleaved stereo at the device rate.
pub struct PcmWindow {
    /// The frame `samples` starts at.
    pub start: u64,
    /// Interleaved stereo. Its length decides how much the window covers.
    pub samples: Vec<f32>,
}

impl PcmWindow {
    pub fn empty() -> Self {
        Self { start: 0, samples: Vec::new() }
    }

    pub fn frames(&self) -> u64 {
        self.samples.len() as u64 / 2
    }

    /// Whether the window still has room either side of `frame` to read from.
    ///
    /// Not merely whether it holds it: a head one frame from the edge is a
    /// head that will be off it before the next block, and refilling takes a
    /// demuxer seek.
    pub fn comfortable(&self, frame: f64, margin: u64) -> bool {
        if self.samples.is_empty() {
            return false;
        }
        let end = self.start + self.frames();
        frame >= (self.start + margin) as f64 && frame + margin as f64 <= end as f64
    }

    /// The stereo pair at a fractional frame, interpolated between neighbours.
    ///
    /// Silence outside what it holds, which is what the start of a track
    /// sounds like when the head is dragged off the front of it.
    pub fn sample(&self, frame: f64) -> (f32, f32) {
        if frame < self.start as f64 {
            return (0.0, 0.0);
        }
        let offset = frame - self.start as f64;
        let index = offset.floor();
        if index < 0.0 || index + 1.0 >= self.frames() as f64 {
            return (0.0, 0.0);
        }
        let i = (index as usize) * 2;
        let t = (offset - index) as f32;
        let left = self.samples.get(i).copied().unwrap_or(0.0);
        let right = self.samples.get(i + 1).copied().unwrap_or(0.0);
        let next_left = self.samples.get(i + 2).copied().unwrap_or(0.0);
        let next_right = self.samples.get(i + 3).copied().unwrap_or(0.0);
        (left + (next_left - left) * t, right + (next_right - right) * t)
    }
}

/// The read head: where it is, where the pointer wants it, and how fast it is
/// travelling to get there.
pub struct Scrubber {
    cursor: f64,
    target: f64,
    rate: f64,
    /// How much of the read head is being heard.
    ///
    /// Ramped rather than switched: see `fade::FADE_FRAMES`.
    gain: Ramp,
    /// Whether the pointer has said anything yet.
    ///
    /// Until it has, the head has nowhere to go and would render silence. The
    /// deck holds off rather than filling the ring with it: a ring of silence
    /// has to play out before the first real sound, which was heard as a gap
    /// at the start of every drag.
    aimed: bool,
    /// Output frames rendered since the pointer last reported.
    ///
    /// Not a flag. Blocks come out faster than a pointer emits moves — 11.6 ms
    /// against a screen frame — so a boolean cleared by the first block after
    /// a move makes the second and third of every pass believe the hand has
    /// stopped. On a slow drag that is silence two blocks in three, which is
    /// heard as a stutter over the vinyl.
    still: f64,
    /// How fast the hand is moving, in frames of music per frame of output.
    ///
    /// Measured between reports and held between them, which is the whole of
    /// the fix for a slow drag. A mouse reports a whole pixel at a time, so
    /// how *far* the pointer has moved says nothing on its own about how fast
    /// it was going to get there — a pixel is a pixel whether the hand took
    /// ten milliseconds over it or a hundred. The time it took is what sets
    /// the pitch, exactly as the speed of a hand on a record does.
    speed: f64,
    /// Output frames between the last two reports, smoothed.
    ///
    /// The hand's own clock: how far behind the pointer the head should sit,
    /// and how long a silence means the hand has stopped. Zero until a report
    /// has been timed; `interval` is what reads it.
    interval: f64,
    /// Reports the pointer has sent, up to the point where it stops mattering.
    ///
    /// The first one only says where the pointer is: there is nothing before
    /// it to measure a speed against, and the distance from wherever the head
    /// happened to be is not one. Until a second has arrived there is no
    /// measured speed, so the distance is all the head has to go on and it is
    /// followed at full strength — which is the old loop, kept for exactly as
    /// long as it is the best available answer. The second report is taken
    /// whole and the ones after it are smoothed.
    reports: u32,
    /// Frames in a block, as the last one came in.
    ///
    /// The settle thresholds are naturally expressed in blocks and everything
    /// else in frames, so one has to be converted into the other.
    block: f64,
}

impl Scrubber {
    pub fn new(at: u64) -> Self {
        Self {
            cursor: at as f64,
            target: at as f64,
            rate: 0.0,
            still: 0.0,
            speed: 0.0,
            interval: 0.0,
            reports: 0,
            block: 512.0,
            aimed: false,
            gain: Ramp::silent(),
        }
    }

    /// Where the pointer is now.
    ///
    /// The head is pulled up behind it if it has fallen further than
    /// `MAX_LAG`: what it skips over is not rendered, which is the difference
    /// between fast-forwarding and grinding.
    pub fn aim(&mut self, frame: u64, since: f64) {
        // Both sides are whole frames that came in as integers, so this is a
        // comparison of exact values rather than of two computed floats: the
        // pointer either sent a new frame or repeated the last one.
        let to = frame as f64;
        let moved = to - self.target;
        if moved.abs() >= 1.0 {
            // How long the hand took over that, in output frames, off the
            // clock the report was stamped with. Counting the blocks rendered
            // in between instead rounds the gap to 11.6 ms, and a hand
            // reporting every 33 ms then reads as moving at five sixths or
            // seven sixths of its real speed on alternate reports — which is
            // audible on anything sustained. Falling back on the block count
            // only covers a caller that has no clock to stamp with.
            let over = if since.is_finite() && since > 0.0 {
                since
            } else {
                self.still.max(self.block)
            };
            // Clamped where it is measured, not only where it is used. A
            // flick crosses thirty seconds of music in a fifth of a second,
            // which is a hand moving at over a hundred times playback; the
            // head cannot run at that, so carrying the number around only lets
            // it inflate everything worked out from it — the trail, and with
            // it how far behind the head is allowed to fall.
            let seen = (moved / over).clamp(-MAX_RATE, MAX_RATE);
            if self.reports > 1 {
                self.speed += (seen - self.speed) * SPEED_SMOOTH;
                self.interval += (over - self.interval) * INTERVAL_SMOOTH;
            } else if self.reports == 1 {
                // Taken whole, not smoothed up from nothing. Smoothing needs
                // something to smooth towards, and until the second report
                // there is nothing: starting from zero and filtering meant a
                // hand reporting every 100 ms was read as reporting every 30
                // for the first half second of the drag, and then read as
                // having stopped between its own reports.
                self.speed = seen;
                self.interval = over;
            }
            self.reports = self.reports.saturating_add(1);
            self.still = 0.0;
        }
        self.target = to;
        self.aimed = true;
        self.pull_up();
    }

    /// Pulls the head up if it has fallen further behind than it may be.
    ///
    /// Measured from where the head ought to be, not from the pointer. A hand
    /// moving at three times playback belongs 5 000 frames behind, so a flat
    /// cap of `MAX_LAG` was pulling the head up on every report of an ordinary
    /// fast drag and holding it to nine tenths of the speed of the hand. What
    /// the cap is for is the gap a flick opens *beyond* the trail, which no
    /// rate could ever close.
    fn pull_up(&mut self) {
        // One report's worth of trail, not the whole of it. The ring's own
        // lead is 2 blocks, which at the top rate is another 8 000 frames the
        // head would be entitled to grind through after a flick before the
        // stop is noticed — and a flick that keeps sounding is the thing
        // `MAX_LAG` exists to prevent.
        let limit = MAX_LAG + (self.speed * self.interval()).abs();
        let lag = self.target - self.cursor;
        if lag > limit {
            self.cursor = self.target - limit;
        } else if lag < -limit {
            self.cursor = self.target + limit;
        }
    }

    /// Whether the pointer has moved at all since the drag began.
    pub fn started(&self) -> bool {
        self.aimed
    }

    pub fn cursor(&self) -> u64 {
        self.cursor.max(0.0) as u64
    }

    /// Where the pointer left it, which is not where the head got to.
    ///
    /// The head is rate-limited — `MAX_RATE` — because a drag has to stay
    /// audible, so a fast one leaves it seconds behind the hand and a click
    /// barely moves it at all. Letting go is a statement about the pointer,
    /// not about the audio that was still catching up, so this is what the
    /// deck lands on.
    pub fn target(&self) -> u64 {
        self.target.max(0.0) as u64
    }

    /// The speed the head is travelling at, as a multiple of normal. Read by
    /// the tests, and by anything that wants to draw the drag.
    #[cfg_attr(not(test), allow(dead_code, reason = "read by the scrub tests"))]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// How far behind the pointer the head belongs, in frames.
    ///
    /// One report plus the ring's own lead, at the speed the hand is moving,
    /// so that a hand reporting every hundred milliseconds still has a hundred
    /// milliseconds of music in hand to play.
    fn trail(&self) -> f64 {
        self.speed * (self.interval() + LOOKAHEAD * self.block)
    }

    /// Output frames between the pointer's reports, or what to assume before
    /// one has been timed.
    fn interval(&self) -> f64 {
        if self.interval > 0.0 {
            self.interval
        } else {
            INITIAL_INTERVAL * self.block
        }
    }

    /// Chooses the rate for the next `frames` of output.
    ///
    /// The distance left to the pointer *is* the speed: a hand that moved half
    /// a second of music in the last block wants half a second of music played
    /// in the next one, which is what makes the pitch follow the drag.
    pub fn plan(&mut self, frames: usize) -> f64 {
        self.block = (frames as f64).max(1.0);
        self.still += self.block;
        let span = (self.block * LOOKAHEAD).max(1.0);
        // Quiet for a couple of its own intervals: the hand has stopped rather
        // than gone slow, so the speed it was carrying is no longer true.
        let stopped = self.still > self.interval() * STOPPED_INTERVALS + SETTLE_BLOCKS * self.block;
        if stopped {
            self.speed = 0.0;
            // The trail goes with the speed, and so does what the head is
            // allowed to be behind by. Applying that here as well as in `aim`
            // is what ends a flick: the last report of one leaves the head the
            // better part of a second's worth of music behind, and without
            // this there is no further report to pull it up — it grinds
            // through all of it at `MAX_RATE` after the hand has stopped.
            self.pull_up();
            // Arrived as well, so there is nothing left to play. Snapping
            // rather than converging is what makes the sound stop: an
            // exponential approach spends half a second getting quiet, which
            // is heard as the drag carrying on after the hand has.
            if (self.target - self.cursor).abs() < span {
                // The rate stops; the head does not move. Closing the last of
                // the gap by jumping is a step in the waveform, and on bass
                // that is a crack. What is left is under a block, and letting
                // go lands on the pointer rather than on the head anyway.
                self.rate = 0.0;
                return 0.0;
            }
        }
        // Where the pointer will have got to by now, not where it last said
        // it was. The hand keeps moving between reports, so the gap to the
        // last reported position is a sawtooth — it grows for a whole report
        // interval and then drops — and correcting against a sawtooth puts one
        // into the rate. Carrying the report forward at the speed the hand was
        // measured at takes it out.
        // Carried no further than the next report was due. Past that the hand
        // has said nothing about where it is, and a hand that has stopped is
        // exactly the case where guessing it kept moving is wrong: the head
        // would chase a pointer that is not there until the stop is noticed.
        let expected = self.target + self.speed * self.still.min(self.interval());
        let drift = (expected - self.cursor) - self.trail();
        // The hand's speed, with a gentle pull back onto where the head ought
        // to be by now. The pull is what keeps a measurement that is slightly
        // off from becoming a head a second behind the pointer; it is weak
        // because a strong one is the sprint this replaced.
        // Full strength while there is no measured speed to lead with, and
        // once the hand has stopped: the sprint this replaced was a problem
        // only because more reports were coming behind it. A shove that has
        // ended should be played out and finished, not crawled through.
        let correct = if self.reports > 1 && !stopped { CORRECT } else { 1.0 };
        let wanted = (self.speed + drift / span * correct).clamp(-MAX_RATE, MAX_RATE);
        self.rate += (wanted - self.rate) * SMOOTH;
        if self.rate.abs() < REST_RATE {
            self.rate = 0.0;
        }
        self.rate
    }

    /// Fills `out` from the window at the planned rate, moving the head.
    ///
    /// Returns the frames written. A head at rest writes silence rather than
    /// nothing: the block still has to be produced, or the callback runs dry
    /// and the deck sounds broken instead of stopped.
    pub fn render(&mut self, window: &PcmWindow, out: &mut [f32]) -> usize {
        let frames = out.len() / 2;
        let rate = self.plan(frames);
        for i in 0..frames {
            // The head cannot get in front of the hand. A record only turns
            // as far as it has been pushed, and holding the head at the
            // pointer is what brings a drag to rest the moment the hand does:
            // it plays out the music it was trailing by and then has none
            // left, rather than running on past where the pointer stopped.
            let arrived = (rate > 0.0 && self.cursor >= self.target)
                || (rate < 0.0 && self.cursor <= self.target);
            // A moving head is heard, a resting one is faded out. The fade is
            // what keeps a stop from being a step: the waveform is wherever it
            // is when the hand pauses, and cutting it dead is a click.
            let gain = self.gain.step(rate != 0.0 && !arrived);
            // Sampled even at rest, so the fade has the waveform to fade out
            // rather than an abrupt zero. A held sample reaching zero is a
            // decay; a held sample held is the hum `REST_RATE` guards against.
            let (left, right) = window.sample(self.cursor);
            if let Some(slot) = out.get_mut(i * 2) {
                *slot = left * gain;
            }
            if let Some(slot) = out.get_mut(i * 2 + 1) {
                *slot = right * gain;
            }
            if !arrived {
                self.cursor = (self.cursor + rate).max(0.0);
            }
        }
        frames
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_wrap,
    reason = "the rate is set to exactly zero at rest, and these are test assertions"
)]
mod tests {
    use super::*;
    use crate::fade::FADE_FRAMES;

    /// A window whose left channel is its own frame number, so what came out
    /// says where it was read from.
    fn ramp(start: u64, frames: u64) -> PcmWindow {
        let mut samples = Vec::with_capacity(frames as usize * 2);
        for i in 0..frames {
            samples.push((start + i) as f32);
            samples.push(0.0);
        }
        PcmWindow { start, samples }
    }

    #[test]
    fn a_sample_between_two_frames_is_between_their_values() {
        let window = ramp(100, 10);
        assert!((window.sample(103.0).0 - 103.0).abs() < 1e-4);
        assert!((window.sample(103.25).0 - 103.25).abs() < 1e-4);
    }

    #[test]
    fn letting_go_lands_under_the_pointer_not_behind_it() {
        // A click on the overview aims seconds away and is over in a frame or
        // two. The head cannot render that distance and does not try: it is
        // pulled up to within `MAX_LAG`, plays that, and the deck lands on the
        // pointer. Landing on the head instead made the press spring back.
        let mut scrubber = Scrubber::new(1_000);
        scrubber.aim(2_000_000, 0.0);
        let window = ramp(0, 8_000);
        let mut out = vec![0.0; 512];
        scrubber.render(&window, &mut out);
        assert!(
            (2_000_000.0 - scrubber.cursor() as f64) <= MAX_LAG,
            "the head follows the pointer rather than grinding after it: {}",
            scrubber.cursor(),
        );
        assert_eq!(scrubber.target(), 2_000_000);
    }

    #[test]
    fn outside_the_window_is_silence_rather_than_the_nearest_frame() {
        // Dragged off the front of a track you hear nothing, not the first
        // frame held forever.
        let window = ramp(100, 10);
        assert_eq!(window.sample(99.0), (0.0, 0.0));
        assert_eq!(window.sample(120.0), (0.0, 0.0));
        assert_eq!(PcmWindow::empty().sample(0.0), (0.0, 0.0));
    }

    #[test]
    fn a_head_near_the_edge_is_not_comfortable_there() {
        let window = ramp(100, 1000);
        assert!(window.comfortable(600.0, 100));
        assert!(!window.comfortable(150.0, 100));
        assert!(!window.comfortable(1050.0, 100));
        assert!(!PcmWindow::empty().comfortable(0.0, 1));
    }

    #[test]
    fn the_rate_follows_the_distance_to_the_pointer() {
        // A pointer that has run ahead pulls the head after it; one that has
        // not moved lets it come to rest.
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(10_000, 0.0);
        let first = scrubber.plan(512);
        assert!(first > 0.0, "{first}");
        let mut last = first;
        for _ in 0..8 {
            last = scrubber.plan(512);
        }
        assert!(last >= first, "the rate should build, not decay: {first} to {last}");
    }

    #[test]
    fn dragging_backwards_plays_backwards() {
        let mut scrubber = Scrubber::new(10_000);
        scrubber.aim(0, 0.0);
        assert!(scrubber.plan(512) < 0.0);
    }

    #[test]
    fn a_slow_drag_is_never_pulled_up_and_stays_continuous() {
        // The cap is for flicks. A hand moving at anything like playback speed
        // never reaches it, so nothing is skipped and the sound is unbroken.
        let mut scrubber = Scrubber::new(0);
        let window = ramp(0, 44_100 * 4);
        let mut out = vec![0.0_f32; 512 * 2];
        let mut at = 0_u64;
        for _ in 0..20 {
            at += 512;
            scrubber.aim(at, 512.0);
            scrubber.render(&window, &mut out);
            assert!(out.iter().any(|s| *s != 0.0), "a slow drag should not go quiet");
        }
    }

    /// A 60 Hz sine, which is what a kick drum looks like to the read head.
    fn bass(start: u64, frames: u64) -> PcmWindow {
        let mut samples = Vec::with_capacity(frames as usize * 2);
        for i in 0..frames {
            let t = (start + i) as f64 / 44_100.0;
            let value = (t * 60.0 * std::f64::consts::TAU).sin() as f32;
            samples.push(value);
            samples.push(value);
        }
        PcmWindow { start, samples }
    }

    /// The largest step between neighbouring output samples.
    ///
    /// Across the whole stream, not within a block: the join between one block
    /// and the next is exactly where a click hides.
    fn worst_step(out: &[f32]) -> f32 {
        out.chunks_exact(2)
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max)
    }

    #[test]
    fn a_slow_drag_over_bass_does_not_step() {
        // Crackle is a discontinuity, and bass is where it is loudest: a big
        // smooth waveform makes any jump in the read position obvious. At 60 Hz
        // and about playback speed, neighbouring samples differ by well under
        // a hundredth; anything above that came from the head, not the music.
        let window = bass(0, 44_100 * 4);
        let mut scrubber = Scrubber::new(0);
        let mut out = vec![0.0_f32; 512 * 2];
        let mut at = 0_u64;
        let mut stream: Vec<f32> = Vec::new();
        for pass in 0..120 {
            // A hand: moving, then resting long enough for the head to settle,
            // then moving on. A hand does that constantly, and the settle is
            // where a jump would come from.
            if pass % 12 < 5 {
                at += 600;
                scrubber.aim(at, 0.0);
            }
            scrubber.render(&window, &mut out);
            stream.extend_from_slice(&out);
        }
        let worst = worst_step(&stream);
        assert!(worst < 0.02, "the head stepped by {worst}, which is a click");
    }

    #[test]
    fn a_drag_stays_audible_between_pointer_moves() {
        // The one that bit. Blocks come out every 11.6 ms and a pointer moves
        // about once a screen frame, so most blocks are planned without a new
        // target. Treating "no move since the last block" as "the hand has
        // stopped" silenced two blocks in three, which is heard as a stutter
        // over what should be an unbroken vinyl sound.
        let window = ramp(0, 44_100 * 4);
        let mut scrubber = Scrubber::new(0);
        let mut out = vec![0.0_f32; 512 * 2];
        let mut at = 0_u64;
        let mut silent = 0;
        for pass in 0..30 {
            // A move every third block, which is slower than a real pointer.
            if pass % 3 == 0 {
                at += 1_500;
                scrubber.aim(at, 512.0 * 3.0);
            }
            scrubber.render(&window, &mut out);
            if out.iter().all(|s| *s == 0.0) {
                silent += 1;
            }
        }
        assert_eq!(silent, 0, "{silent} of 30 blocks went quiet mid-drag");
    }

    /// A steady hand should turn the record at a steady speed.
    ///
    /// The one the report was about: scrubbing slowly sounded choppy rather
    /// than like a record being moved. The distance left to the pointer is a
    /// sawtooth — the target jumps once a screen frame and the head eats into
    /// the gap over the two blocks before the next jump — so the rate rose and
    /// fell by 9% of itself under a hand moving perfectly evenly, which is a
    /// warble on anything sustained. `SMOOTH` carries the measurement; this
    /// keeps it honest.
    #[test]
    fn a_steady_hand_turns_the_record_at_a_steady_speed() {
        const RATE: f64 = 44_100.0;
        const BLOCK: usize = 512;
        // A pointer move every screen frame, which is what the app coalesces
        // to, at speeds from a full turn down to a crawl.
        for speed in [1.0_f64, 0.5, 0.2, 0.05] {
            let frames = (RATE * 3.0 * speed.max(1.0) + RATE) as u64;
            let window = ramp(0, frames);
            let mut scrubber = Scrubber::new(0);
            let mut out = vec![0.0_f32; BLOCK * 2];

            let block_ms = BLOCK as f64 / RATE * 1000.0;
            let (mut now, mut next_move, mut pointer_ms) = (0.0_f64, 0.0_f64, 0.0_f64);
            let mut rates = Vec::new();
            for _ in 0..(3000.0 / block_ms) as usize {
                while next_move <= now {
                    pointer_ms += speed * 16.7;
                    // Fractional, as `scrub_to_ms` now takes it: rounding this
                    // to a whole millisecond is 44 frames of quantisation and
                    // takes the spread below from 4% to 38% at 0.05x.
                    scrubber.aim((pointer_ms / 1000.0 * RATE) as u64, 16.7 / 1000.0 * RATE);
                    next_move += 16.7;
                }
                scrubber.render(&window, &mut out);
                rates.push(scrubber.rate());
                now += block_ms;
            }

            // Once it has reached speed, ignoring the ramp up to it.
            let settled: Vec<f64> = rates.iter().skip(60).copied().collect();
            let mean = settled.iter().sum::<f64>() / settled.len() as f64;
            let sd = (settled.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / settled.len() as f64)
                .sqrt();
            assert!(
                (mean - speed).abs() < speed * 0.1,
                "at {speed}x the head averaged {mean}",
            );
            let spread = sd / mean.abs() * 100.0;
            // Measured at 3.7%. Was 9.0%. The bound is where a regression in
            // the smoothing shows up rather than where the ear gives out.
            assert!(spread < 5.0, "at {speed}x the rate wandered by {spread:.1}% of itself");
            assert!(
                rates.iter().skip(60).all(|r| *r != 0.0),
                "at {speed}x the head stopped mid-drag",
            );
        }
    }

    /// A hand crossing pixels slowly still turns the record.
    ///
    /// The one the second report was about: dragging slowly sounded like the
    /// track playing at its own pitch in bursts with gaps between. A mouse
    /// only reports once the cursor has crossed a whole pixel, and a pixel of
    /// the detail waveform at its default zoom is 1102 frames — about one
    /// `span`. So under the old loop every pixel crossed commanded a full
    /// speed sprint however long the hand had taken to cross it, and then the
    /// settle timer expired before the next pixel arrived and cut the head to
    /// silence. Below about 30 px/s it was quiet more than half the time.
    ///
    /// `a_steady_hand_turns_the_record_at_a_steady_speed` missed it because it
    /// models a pointer reporting a fresh fractional position every screen
    /// frame, which is a trackpad rather than a mouse.
    #[test]
    fn a_hand_crossing_pixels_slowly_still_turns_the_record() {
        const RATE: f64 = 44_100.0;
        const BLOCK: usize = 512;
        const BLOCK_SECONDS: f64 = BLOCK as f64 / RATE;
        const POINTER_HZ: f64 = 60.0;
        /// Twelve bars at 128 BPM is 22.5 s, over a strip about 900 px wide.
        const FRAMES_PER_PIXEL: f64 = 22.5 * RATE / 900.0;

        for px_per_second in [10.0_f64, 20.0, 30.0, 50.0, 120.0] {
            let start = 1_000_000.0_f64;
            let window = ramp(0, 2_000_000);
            let mut scrubber = Scrubber::new(start as u64);
            let mut out = vec![0.0_f32; BLOCK * 2];
            let mut rates = Vec::new();
            let (mut reported, mut next_report, mut last_report) = (0.0_f64, 0.0_f64, 0.0_f64);
            for block in 0..(2.0 / BLOCK_SECONDS) as usize {
                let now = block as f64 * BLOCK_SECONDS;
                while next_report <= now {
                    let exact = (next_report * px_per_second).floor();
                    // Whole pixels, and only once one has been crossed.
                    if (exact - reported).abs() >= 1.0 {
                        reported = exact;
                        // Stamped when the pointer reported, which is what the
                        // engine does — not when the block that follows it is
                        // rendered.
                        let since = (next_report - last_report) * RATE;
                        last_report = next_report;
                        scrubber.aim((start + reported * FRAMES_PER_PIXEL) as u64, since);
                    }
                    next_report += 1.0 / POINTER_HZ;
                }
                scrubber.render(&window, &mut out);
                // Past the quarter second the head takes to reach speed.
                if now > 0.25 {
                    rates.push(scrubber.rate());
                }
            }

            let hand = px_per_second * FRAMES_PER_PIXEL / RATE;
            assert!(
                rates.iter().all(|r| *r != 0.0),
                "at {px_per_second} px/s the head went quiet mid-drag",
            );
            let mean = rates.iter().sum::<f64>() / rates.len() as f64;
            // Measured within 6.5%, and within 1% at the speeds a hand
            // spends most of its time at. The pull back onto the pointer is
            // deliberately weak, and a steady few percent off is a pitch
            // nobody hears as wrong — see `CORRECT`.
            assert!(
                (mean - hand).abs() < hand * 0.10,
                "a {hand}x hand turned the record at {mean}x",
            );
            let sd = (rates.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / rates.len() as f64)
                .sqrt();
            let warble = sd / mean * 100.0;
            // Measured at 4.2% averaged over these speeds and 6.8% at the
            // worst of them, against 117.7% at 10 px/s before. The bound is
            // where a regression shows up, not where the ear gives out.
            assert!(
                warble < 10.0,
                "at {px_per_second} px/s the rate wandered by {warble:.1}% of itself",
            );
        }
    }

    #[test]
    fn the_head_still_settles_once_the_moves_stop() {
        // And the other half: a hand that has genuinely stopped goes quiet
        // within a few blocks rather than grinding on.
        let window = ramp(0, 44_100 * 4);
        let mut scrubber = Scrubber::new(0);
        let mut out = vec![0.0_f32; 512 * 2];
        scrubber.aim(2_000, 0.0);
        for _ in 0..(SETTLE_BLOCKS as u32 + 6) {
            scrubber.render(&window, &mut out);
        }
        assert_eq!(scrubber.rate(), 0.0, "the record should have stopped");
        assert!(out.iter().all(|s| *s == 0.0), "and gone quiet");
    }

    #[test]
    fn a_flick_leaves_the_head_close_behind_rather_than_seconds_behind() {
        // A fast drag moves the pointer further than the head can render. It
        // plays the last stretch and skips the rest: grinding through at eight
        // times means audio still running long after the hand has stopped.
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(44_100 * 30, 0.0);
        assert!(
            (scrubber.target() as f64 - scrubber.cursor() as f64) <= MAX_LAG,
            "left {} frames behind",
            scrubber.target() - scrubber.cursor(),
        );
    }

    #[test]
    fn dragging_back_fast_pulls_the_head_back_too() {
        let mut scrubber = Scrubber::new(44_100 * 30);
        scrubber.aim(0, 0.0);
        assert!((scrubber.cursor() as f64) <= MAX_LAG, "at {}", scrubber.cursor());
    }

    #[test]
    fn a_pointer_that_stops_lets_the_head_reach_it_and_go_quiet() {
        // The whole shape of a fast drag: a burst, then silence, and the head
        // where the pointer is rather than somewhere behind it.
        let window = ramp(0, 44_100 * 8);
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(44_100 * 2, 0.0);
        let mut out = vec![0.0_f32; 512 * 2];
        let mut sounding = 0;
        for _ in 0..40 {
            scrubber.render(&window, &mut out);
            if out.iter().any(|s| *s != 0.0) {
                sounding += 1;
            }
        }
        assert!(sounding > 0, "a drag should be audible");
        assert_eq!(scrubber.rate(), 0.0, "and then stop");
        assert!(out.iter().all(|s| *s == 0.0), "the last block should be silent");
    }

    #[test]
    fn a_flick_is_clamped_rather_than_becoming_noise() {
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(u64::from(u32::MAX), 0.0);
        for _ in 0..50 {
            scrubber.plan(512);
        }
        assert!(scrubber.rate() <= MAX_RATE + 1e-6, "{}", scrubber.rate());
    }

    #[test]
    fn a_pointer_that_stops_brings_the_head_to_rest() {
        // The head only moves as blocks are produced, so this has to render
        // rather than plan: planning alone leaves the distance unchanged.
        let window = ramp(0, 40_000);
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(5_000, 0.0);
        let mut out = vec![0.0_f32; 512 * 2];
        for _ in 0..60 {
            scrubber.render(&window, &mut out);
        }
        assert_eq!(scrubber.rate(), 0.0, "the record should have stopped");
        // Within the gap `plan` settles inside, which is `LOOKAHEAD` blocks —
        // it stops the rate rather than jumping the last of the distance,
        // because a jump is a step in the waveform and on bass that is a
        // crack. This asserted 200 before, which was tighter than the code
        // ever promised: it held only because the old rate smoothing got the
        // head there sooner. Nothing is lost by the gap — `scrub_end` lands
        // the deck on the pointer, not on the head.
        let short = (scrubber.cursor() as i64 - 5_000).abs();
        assert!(short <= 512 * LOOKAHEAD as i64, "{short} frames short, at {}", scrubber.cursor());
    }

    #[test]
    fn rendering_reads_the_window_where_the_head_is() {
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(1_000);
        scrubber.aim(1_000 + 512 * 2, 0.0);
        let mut out = vec![0.0_f32; 512 * 2];
        assert_eq!(scrubber.render(&window, &mut out), 512);
        // Past the fade-in, which is what keeps a start from being a step: the
        // head is reading the window from where it was told to.
        let past_fade = usize::from(FADE_FRAMES) * 2;
        assert!(out[past_fade] > 1_000.0, "{}", out[past_fade]);
        assert!(out[1022] > out[past_fade], "the block should move forwards");
    }

    #[test]
    fn a_head_at_rest_writes_silence_rather_than_a_held_sample() {
        // A held sample is a click and then a hum; a stopped record is quiet.
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(1_000);
        scrubber.aim(1_000, 0.0);
        let mut out = vec![9.0_f32; 512 * 2];
        scrubber.render(&window, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn the_head_never_runs_before_the_start_of_the_track() {
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(100);
        scrubber.aim(0, 0.0);
        let mut out = vec![0.0_f32; 512 * 2];
        for _ in 0..20 {
            scrubber.render(&window, &mut out);
        }
        // Never past the front of the track. It rests a little short of the
        // target rather than exactly on it — closing that last gap by jumping
        // is the step this deliberately does not make.
        assert!(scrubber.cursor() < 1_024, "at {}", scrubber.cursor());
    }
}
