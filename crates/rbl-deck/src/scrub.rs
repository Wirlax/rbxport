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

/// Frames either side of the cursor the window holds.
///
/// Four seconds at 44.1 kHz, so a drag has eight seconds of room before the
/// demuxer is asked for anything. Stereo `f32` makes that 2.8 MB while a drag
/// is running and nothing when it is not.
pub const WINDOW_REACH: u64 = 44_100 * 4;

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
const SMOOTH: f64 = 0.4;

/// Blocks of audio the read head aims to be behind the pointer.
///
/// The ring is ahead of the callback, so a head that closed the whole gap in
/// one block would arrive early and then wait. Two blocks is about 23 ms.
const LOOKAHEAD: f64 = 2.0;

/// Below this the record has stopped, and silence is what a stopped record
/// makes. Interpolating at a rate this low is a held sample, which is a click
/// and then a hum.
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
    /// Whether the pointer has moved since the last block was planned.
    ///
    /// The head only snaps to a target that is standing still. A hand moving
    /// at about playback speed keeps the head within a block of the pointer,
    /// and snapping on distance alone would silence exactly the drag that
    /// should sound most like the record.
    moving: bool,
}

impl Scrubber {
    pub fn new(at: u64) -> Self {
        Self { cursor: at as f64, target: at as f64, rate: 0.0, moving: false }
    }

    /// Where the pointer is now.
    ///
    /// The head is pulled up behind it if it has fallen further than
    /// `MAX_LAG`: what it skips over is not rendered, which is the difference
    /// between fast-forwarding and grinding.
    pub fn aim(&mut self, frame: u64) {
        // Both sides are whole frames that came in as integers, so this is a
        // comparison of exact values rather than of two computed floats: the
        // pointer either sent a new frame or repeated the last one.
        let aimed = frame as f64;
        self.moving = (aimed - self.target).abs() >= 1.0;
        self.target = aimed;
        let lag = self.target - self.cursor;
        if lag > MAX_LAG {
            self.cursor = self.target - MAX_LAG;
        } else if lag < -MAX_LAG {
            self.cursor = self.target + MAX_LAG;
        }
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

    /// Chooses the rate for the next `frames` of output.
    ///
    /// The distance left to the pointer *is* the speed: a hand that moved half
    /// a second of music in the last block wants half a second of music played
    /// in the next one, which is what makes the pitch follow the drag.
    pub fn plan(&mut self, frames: usize) -> f64 {
        // Arrived, and the hand has stopped. Snapping rather than converging
        // is what makes the sound stop: an exponential approach spends half a
        // second getting quiet, which is heard as the drag carrying on after
        // the hand has. A pointer that is still moving is followed instead,
        // however close it is.
        let moving = self.moving;
        self.moving = false;
        if !moving && (self.target - self.cursor).abs() < frames as f64 * LOOKAHEAD {
            self.cursor = self.target;
            self.rate = 0.0;
            return 0.0;
        }
        let span = (frames as f64 * LOOKAHEAD).max(1.0);
        let wanted = ((self.target - self.cursor) / span).clamp(-MAX_RATE, MAX_RATE);
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
            let (left, right) = if rate == 0.0 { (0.0, 0.0) } else { window.sample(self.cursor) };
            if let Some(slot) = out.get_mut(i * 2) {
                *slot = left;
            }
            if let Some(slot) = out.get_mut(i * 2 + 1) {
                *slot = right;
            }
            self.cursor = (self.cursor + rate).max(0.0);
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
        scrubber.aim(2_000_000);
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
        scrubber.aim(10_000);
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
        scrubber.aim(0);
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
            scrubber.aim(at);
            scrubber.render(&window, &mut out);
            assert!(out.iter().any(|s| *s != 0.0), "a slow drag should not go quiet");
        }
    }

    #[test]
    fn a_flick_leaves_the_head_close_behind_rather_than_seconds_behind() {
        // A fast drag moves the pointer further than the head can render. It
        // plays the last stretch and skips the rest: grinding through at eight
        // times means audio still running long after the hand has stopped.
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(44_100 * 30);
        assert!(
            (scrubber.target() as f64 - scrubber.cursor() as f64) <= MAX_LAG,
            "left {} frames behind",
            scrubber.target() - scrubber.cursor(),
        );
    }

    #[test]
    fn dragging_back_fast_pulls_the_head_back_too() {
        let mut scrubber = Scrubber::new(44_100 * 30);
        scrubber.aim(0);
        assert!((scrubber.cursor() as f64) <= MAX_LAG, "at {}", scrubber.cursor());
    }

    #[test]
    fn a_pointer_that_stops_lets_the_head_reach_it_and_go_quiet() {
        // The whole shape of a fast drag: a burst, then silence, and the head
        // where the pointer is rather than somewhere behind it.
        let window = ramp(0, 44_100 * 8);
        let mut scrubber = Scrubber::new(0);
        scrubber.aim(44_100 * 2);
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
        scrubber.aim(u64::from(u32::MAX));
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
        scrubber.aim(5_000);
        let mut out = vec![0.0_f32; 512 * 2];
        for _ in 0..60 {
            scrubber.render(&window, &mut out);
        }
        assert_eq!(scrubber.rate(), 0.0, "the record should have stopped");
        assert!((scrubber.cursor() as i64 - 5_000).abs() < 200, "at {}", scrubber.cursor());
    }

    #[test]
    fn rendering_reads_the_window_where_the_head_is() {
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(1_000);
        scrubber.aim(1_000 + 512 * 2);
        let mut out = vec![0.0_f32; 512 * 2];
        assert_eq!(scrubber.render(&window, &mut out), 512);
        // Normal-ish speed forwards: the first sample is where the head was.
        assert!((out[0] - 1_000.0).abs() < 2.0, "{}", out[0]);
        assert!(out[1022] > out[0], "the block should move forwards");
    }

    #[test]
    fn a_head_at_rest_writes_silence_rather_than_a_held_sample() {
        // A held sample is a click and then a hum; a stopped record is quiet.
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(1_000);
        scrubber.aim(1_000);
        let mut out = vec![9.0_f32; 512 * 2];
        scrubber.render(&window, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn the_head_never_runs_before_the_start_of_the_track() {
        let window = ramp(0, 4_000);
        let mut scrubber = Scrubber::new(100);
        scrubber.aim(0);
        let mut out = vec![0.0_f32; 512 * 2];
        for _ in 0..20 {
            scrubber.render(&window, &mut out);
        }
        assert_eq!(scrubber.cursor(), 0);
    }
}
