//! The engine end to end, on a sink with no device behind it.
//!
//! Every one of these drives the same audio callback the real device does, so
//! what they assert about position, seeking and silence is what a deck
//! actually produces.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rbl_deck::{Deck, DeckEvent, Engine, NullSink, Sink, FADE_FRAMES};

const RATE: u32 = 44_100;

/// Frames in a deck's ring, which is what the engine's own `RING_BLOCKS`
/// times `BLOCK_FRAMES` comes to.
const RING: usize = 16 * 512;

/// A 16-bit PCM WAV, so no fixture file is needed.
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
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

/// A track whose sample at frame `n` says what `n` is, so a test can read the
/// output and say where in the file it came from.
fn ramp_at(path: &Path, rate: u32, frames: usize) {
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        // 0.5 at frame 0 rising to 1.0 at the end, in both channels.
        let value = 0.5 + 0.5 * (frame as f32 / frames as f32);
        samples.push(value);
        samples.push(value);
    }
    write_wav(path, rate, 2, &samples);
}

fn ramp(path: &Path, frames: usize) {
    ramp_at(path, RATE, frames);
}

/// A track that is one steady loud value, so any step in the output came from
/// the transport rather than from the music.
///
/// 0.8 is most of full scale: it is what a kick drum is doing at the moment
/// somebody presses pause, and a cut from 0.8 to silence in one sample is the
/// click all of this exists to prevent.
const FLAT: f32 = 0.8;

fn flat(path: &Path, frames: usize) {
    write_wav(path, RATE, 2, &vec![FLAT; frames * 2]);
}

/// The largest jump between neighbouring output samples, on the left channel.
///
/// A click is a discontinuity and nothing else, so this is the whole of what
/// "does it click" means. A fade over `FADE_FRAMES` moves at most one
/// eighty-eighth of full scale a frame, so anything much above that is a step
/// somebody would hear.
fn worst_step(out: &[f32]) -> f32 {
    out.chunks_exact(2)
        .map(|frame| frame[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f32, f32::max)
}

/// How long the channel strip takes to go quiet after its input has.
///
/// Its crossovers are four-pole and one of them corners at 300 Hz, and no
/// filter stops faster than a few cycles of its own corner: measured at 186
/// frames, about four milliseconds, from the last audio in to −80 dB out. A
/// real mixer's EQ does the same thing — it is latency, not a defect.
///
/// Measured at 271 frames, about six milliseconds, from the end of a
/// two-millisecond fade to the last sample above −80 dB.
const STRIP_TAIL: usize = 320;

/// Below this nothing is audible: −80 dB of full scale.
///
/// "Silent" is this rather than a hard zero because the channel strip's
/// filters ring on after the audio into them has stopped — an IIR's tail never
/// truly ends. It is 10,000 times below what the fade it follows started at.
const INAUDIBLE: f32 = 1e-4;

/// The most a fade is allowed to move in one frame, with room for the sample
/// rate conversion, the 16-bit quantisation of the fixture, and the channel
/// strip.
///
/// The strip is flat in level but not in phase, and its four-pole crossovers
/// overshoot the edges of a two-millisecond ramp: 0.0147 measured against the
/// ramp's own 0.0091 a frame. Twice the ramp's slope is the allowance, which
/// is still forty times below the step a real cut would make.
fn step_limit() -> f32 {
    FLAT / f32::from(FADE_FRAMES) * 2.0 + 0.005
}

struct Harness {
    engine: Engine,
    sink: Arc<NullSink>,
    events: Arc<Mutex<Vec<String>>>,
    loaded: Arc<AtomicU32>,
}

fn harness() -> Harness {
    let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let loaded = Arc::new(AtomicU32::new(0));
    let seen = Arc::clone(&events);
    let counted = Arc::clone(&loaded);
    let sink_slot: Arc<Mutex<Option<Arc<NullSink>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&sink_slot);

    let sink_events: rbl_deck::EventSink = Arc::new(move |event: DeckEvent| {
        if matches!(event, DeckEvent::Loaded { .. }) {
            counted.fetch_add(1, Ordering::SeqCst);
        }
        seen.lock().unwrap().push(format!("{event:?}"));
    });
    let engine = Engine::with_sink(
        move |render| {
            let sink = Arc::new(NullSink::new(RATE, render));
            *slot.lock().unwrap() = Some(Arc::clone(&sink));
            Ok(sink as Arc<dyn Sink>)
        },
        &sink_events,
    )
    .expect("engine");

    let sink = sink_slot.lock().unwrap().clone().expect("sink");
    Harness { engine, sink, events, loaded }
}

impl Harness {
    /// Waits for the deck to report itself loaded, rather than sleeping a
    /// guessed amount: opening a file is real I/O on another thread.
    fn wait_for_load(&self, count: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.loaded.load(Ordering::SeqCst) < count {
            assert!(Instant::now() < deadline, "the deck never loaded: {:?}", self.events());
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Pulls until the deck's position has moved past `frames`, or gives up.
    fn play_until(&self, deck: Deck, frames: u64) -> Vec<f32> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut out = Vec::new();
        while self.position(deck) < frames {
            out.extend(self.sink.pull(512));
            if Instant::now() > deadline {
                break;
            }
        }
        out
    }

    fn position(&self, deck: Deck) -> u64 {
        match deck {
            Deck::A => self.engine.snapshot().a.position_frames,
            Deck::B => self.engine.snapshot().b.position_frames,
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

#[test]
fn a_loaded_deck_reports_its_length_and_stays_silent_until_it_is_played() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);

    let snapshot = h.engine.snapshot();
    assert!(snapshot.a.loaded);
    assert_eq!(snapshot.a.total_frames, u64::from(RATE));
    assert!(!snapshot.a.playing);
    // Choosing a track must not make a sound, and a stopped device must not
    // even be pulled.
    assert!(!h.sink.running());
    assert!(h.sink.pull(256).iter().all(|s| *s == 0.0));
    assert_eq!(h.position(Deck::A), 0);
}

#[test]
fn playing_moves_the_clock_and_produces_the_files_own_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    assert!(h.sink.running(), "playing must start the device");

    let audio = h.play_until(Deck::A, 4_096);
    assert!(h.position(Deck::A) >= 4_096, "the clock did not advance");
    // The ramp starts at 0.5; silence here would mean the ring never filled.
    let peak = audio.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    assert!(peak > 0.45, "peak was {peak}");
}

#[test]
fn pausing_stops_the_clock_and_the_device() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize * 2);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    h.play_until(Deck::A, 2_048);

    h.engine.pause(Deck::A);
    let at = h.position(Deck::A);
    assert!(!h.sink.running(), "pausing must stop the device");

    // The stream is pulled a moment longer so the deck can fade out — see
    // `LINGER` — and what comes out of it is the fade and then silence.
    let tail = h.sink.pull(2_048);
    let fade = usize::from(FADE_FRAMES);
    assert!(
        tail
            .get((fade + STRIP_TAIL) * 2..)
            .is_some_and(|rest| rest.iter().all(|s| s.abs() < INAUDIBLE)),
        "the deck was still sounding after the fade",
    );
    // The playhead moved by the fade and no further.
    assert!(
        h.position(Deck::A) - at <= u64::from(FADE_FRAMES),
        "the playhead ran on to {} from {at}",
        h.position(Deck::A),
    );
    // And it stays down: the linger is a fade, not a reprieve.
    assert!(h.sink.pull(4_096).iter().all(|s| *s == 0.0));
}

#[test]
fn a_seek_lands_exactly_rather_than_playing_what_was_already_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    // Four seconds, so a seek is far past anything the ring can hold.
    ramp(&path, RATE as usize * 4);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    h.play_until(Deck::A, 2_048);

    let target = u64::from(RATE) * 3;
    h.engine.seek_frames(Deck::A, target);
    // The clock says so at once, before a frame of the new position is played.
    assert_eq!(h.position(Deck::A), target);

    let audio = h.play_until(Deck::A, target + 2_048);
    assert!(h.position(Deck::A) >= target, "the deck went backwards after a seek");

    // Three seconds into a four-second ramp is 0.875, not the 0.5 the ring was
    // holding from the start of the track. Stale blocks would show up here.
    let played: Vec<f32> = audio.into_iter().filter(|s| *s != 0.0).collect();
    let last = played.iter().rev().take(512).fold(0.0_f32, |a, s| a.max(*s));
    assert!(last > 0.8, "after seeking to 3 s the audio was at {last}");
}

#[test]
fn every_jump_lands_on_the_frame_it_asked_for_wherever_it_came_from() {
    // The seek is coarse and then decoded forward, because the demuxer's
    // accurate mode reads the file from a point it knows — seconds, on a long
    // track. What that must not cost is exactness, in either direction: a
    // coarse seek that overshoots is stepped back from, and a format that
    // overshoots anyway pays for the accurate seek instead. The ramp says
    // where audio came from, so a landing that is out reads as a wrong value.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    let frames = RATE as usize * 20;
    ramp(&path, frames);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    h.play_until(Deck::A, 2_048);

    // Forwards to the far end, back to the start, and about the middle.
    for seconds in [19_u64, 1, 10, 2] {
        let target = u64::from(RATE) * seconds;
        h.engine.seek_frames(Deck::A, target);
        assert_eq!(h.position(Deck::A), target, "the clock did not take the seek");

        // Pulled with the decode thread given room to refill, because a null
        // sink drained flat out outruns any decoder.
        let mut out = Vec::new();
        for _ in 0..40 {
            out.extend(h.sink.pull(512));
            std::thread::sleep(Duration::from_millis(2));
        }
        let left: Vec<f32> = out.chunks_exact(2).map(|frame| frame[0]).collect();

        // The end of what came back, against where the playhead says it came
        // from. Not the join — the channel strip's filters smear it, so there
        // is no silent frame to find the new audio by — and not the target
        // either, since the deck has played on since it landed there.
        let heard = left
            .get(left.len().saturating_sub(256)..)
            .expect("nothing was played after the seek")
            .iter()
            .fold(0.0_f32, |a, s| a.max(*s));
        let at = h.position(Deck::A);
        assert!(
            at >= target && at < target + u64::from(RATE),
            "seeking to {seconds} s left the playhead at {at}, not near {target}",
        );
        let want = 0.5 + 0.5 * (at as f32 / frames as f32);
        assert!(
            (heard - want).abs() < 0.01,
            "after seeking to {seconds} s the audio was {heard}, not {want}",
        );
    }
}

#[test]
fn a_seek_while_paused_moves_the_playhead_without_starting_the_device() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize * 2);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);

    h.engine.seek_ms(Deck::A, 1_000);
    assert_eq!(h.position(Deck::A), u64::from(RATE));
    assert!(!h.sink.running());
}

#[test]
fn a_track_that_ends_stops_the_deck_rather_than_running_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.wav");
    ramp(&path, 8_000);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);

    let deadline = Instant::now() + Duration::from_secs(5);
    while h.engine.snapshot().a.playing && Instant::now() < deadline {
        h.sink.pull(512);
    }
    assert!(!h.engine.snapshot().a.playing, "the deck never stopped at the end");
    assert!(h.position(Deck::A) >= 7_500, "it stopped at {}", h.position(Deck::A));
}

#[test]
fn moving_the_master_level_does_not_step() {
    // The level is read once a callback and used for the whole of it, so a
    // hand on the fader arrives as a staircase eleven milliseconds wide —
    // audible on anything loud, and a click on a fast move. Smoothed per
    // frame, the largest step is a fraction of what an ear picks out.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat(&path, RATE as usize * 2);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    // Past the fade in, so what is left is the fader and nothing else.
    h.play_until(Deck::A, RING as u64 + 4_096);

    let mut out = Vec::new();
    // Slammed from full to silence and back, a buffer apart: the worst a hand
    // can do, and further than a hand can actually move.
    for level in [0.0_f32, 1.0, 0.2, 1.0] {
        h.engine.master().set_gain(level);
        out.extend(h.sink.pull(512));
    }
    let worst = worst_step(&out);
    assert!(worst < step_limit(), "the level stepped by {worst}");
}

#[test]
fn the_two_decks_are_independent_and_sum() {
    let dir = tempfile::tempdir().unwrap();
    let one = dir.path().join("one.wav");
    let two = dir.path().join("two.wav");
    write_wav(&one, RATE, 2, &vec![0.4_f32; 44_100 * 2]);
    write_wav(&two, RATE, 2, &vec![0.4_f32; 44_100 * 2]);

    let h = harness();
    h.engine.load(Deck::A, &one);
    h.engine.load(Deck::B, &two);
    h.wait_for_load(2);

    h.engine.play(Deck::A);
    let only_a = h.play_until(Deck::A, 4_096);
    let a_peak = only_a.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    assert!((a_peak - 0.4).abs() < 0.02, "one deck peaked at {a_peak}");

    h.engine.play(Deck::B);
    let both = h.play_until(Deck::B, 4_096);
    let sum_peak = both.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    assert!(sum_peak > 0.7, "two decks summed to {sum_peak}");
    // And B's clock ran on its own rather than following A's.
    assert!(h.position(Deck::B) > 0);
    assert!(h.position(Deck::A) > h.position(Deck::B));
}

#[test]
fn the_device_stops_when_the_last_deck_pauses() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.engine.load(Deck::B, &path);
    h.wait_for_load(2);

    h.engine.play(Deck::A);
    h.engine.play(Deck::B);
    assert!(h.sink.running());
    h.engine.pause(Deck::A);
    // One deck still playing: the device stays up.
    assert!(h.sink.running());
    h.engine.pause(Deck::B);
    assert!(!h.sink.running(), "an idle app must not keep the device running");
}

#[test]
fn a_file_that_cannot_be_decoded_reports_an_error_and_leaves_the_deck_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, b"not audio").unwrap();

    let h = harness();
    h.engine.load(Deck::A, &path);

    let deadline = Instant::now() + Duration::from_secs(5);
    while h.events().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let events = h.events();
    assert!(events.iter().any(|e| e.contains("Error")), "{events:?}");
    assert!(!h.engine.snapshot().a.loaded);
    // And play on an empty deck does nothing at all.
    h.engine.play(Deck::A);
    assert!(!h.engine.snapshot().a.playing);
    assert!(!h.sink.running());
}

/// The budget for a track to be audible after it is asked for, in
/// milliseconds. From the plan's M7-P6 list.
const LOAD_TO_AUDIO_MS: u128 = 200;

#[test]
fn a_track_is_audible_within_the_load_budget() {
    // Load to sound, on the same path the interface uses: `load` returns
    // immediately and the decode thread opens the file, so what is measured is
    // the whole of it — the open, the first blocks, and the fade in.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize * 30);

    let h = harness();
    let asked = Instant::now();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut heard = None;
    while heard.is_none() && Instant::now() < deadline {
        if h.sink.pull(512).iter().any(|s| s.abs() > 0.01) {
            heard = Some(asked.elapsed());
        }
    }
    let took = heard.expect("the deck never made a sound").as_millis();
    assert!(took <= LOAD_TO_AUDIO_MS, "load to audio took {took} ms");
}

#[test]
fn a_deck_played_fast_covers_more_of_the_track_in_the_same_time() {
    // What a tempo control is for. The playhead is in track time, so at +50%
    // the same number of output frames has to cover half as much again of the
    // file — and at −25%, three quarters of it.
    for (tempo, expected) in [(1.5_f32, 1.5_f64), (0.75, 0.75)] {
        for master_tempo in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ramp.wav");
            ramp(&path, RATE as usize * 20);

            let h = harness();
            h.engine.load(Deck::A, &path);
            h.wait_for_load(1);
            h.engine.set_master_tempo(Deck::A, master_tempo);
            h.engine.set_tempo(Deck::A, tempo);
            h.engine.play(Deck::A);

            // A fixed number of output frames, and where the playhead
            // reached. Pulled at about the rate a device would: drained faster
            // than the decode thread can stretch, the ring empties and the
            // playhead measures how fast the test ran rather than the deck.
            let pulls = 60;
            for _ in 0..pulls {
                h.sink.pull(512);
                std::thread::sleep(Duration::from_millis(12));
            }
            let covered = h.position(Deck::A) as f64;
            let out = f64::from(pulls * 512);
            let ratio = covered / out;
            assert!(
                (ratio - expected).abs() < 0.15,
                "at {tempo}x with master tempo {master_tempo} the playhead \
                 covered {ratio:.2} of the track per frame played, not {expected}",
            );
        }
    }
}

#[test]
fn master_tempo_holds_the_pitch_and_without_it_the_pitch_moves() {
    // The difference between the two, measured on the audio rather than
    // asserted: a tone through the deck at +50%, counted by its zero
    // crossings. With the key lock it is the same tone; without it, it is a
    // record played fast.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    tone(&path, RATE as usize * 10);

    let mut heard = Vec::new();
    for master_tempo in [true, false] {
        let h = harness();
        h.engine.load(Deck::A, &path);
        h.wait_for_load(1);
        h.engine.set_master_tempo(Deck::A, master_tempo);
        h.engine.set_tempo(Deck::A, 1.5);
        h.engine.play(Deck::A);

        // Pulled at about the rate a device would, because the frequency is
        // counted over the whole window: drained faster than the decode thread
        // can fill the stretcher, most of it would be silence and the count
        // would measure the gaps rather than the tone.
        let mut out = Vec::new();
        for _ in 0..80 {
            out.extend(h.sink.pull(512));
            std::thread::sleep(Duration::from_millis(12));
        }
        // Past the fade in, and the left channel only.
        let left: Vec<f32> = out.chunks_exact(2).skip(4_096).map(|f| f[0]).collect();
        let sounding = left.iter().filter(|s| s.abs() > 0.05).count();
        assert!(
            sounding * 4 > left.len(),
            "the deck was mostly silent: {sounding} of {} samples",
            left.len(),
        );
        let crossings = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        #[allow(clippy::cast_precision_loss)]
        let hz = crossings as f32 * RATE as f32 / left.len() as f32;
        heard.push(hz);
    }

    let (locked, free) = (heard[0], heard[1]);
    // The fixture's tone is 220 Hz. Locked it stays there; free it goes up by
    // half, which is what a record does.
    assert!((locked - 220.0).abs() < 12.0, "with master tempo the tone was {locked} Hz");
    assert!((free - 330.0).abs() < 18.0, "without it the tone was {free} Hz");
}

#[test]
fn one_deck_going_wrong_leaves_the_other_playing() {
    // The case that matters in front of an audience: a file that will not
    // decode, or one that simply ends, must not take the other deck with it.
    // A stopped device or a frozen playhead on deck B because deck A hit a bad
    // file is the difference between a mistake and a silence.
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("notes.txt");
    std::fs::write(&broken, b"not audio").unwrap();
    let good = dir.path().join("ramp.wav");
    ramp(&good, RATE as usize * 4);

    let h = harness();
    h.engine.load(Deck::B, &good);
    h.wait_for_load(1);
    h.engine.play(Deck::B);
    h.play_until(Deck::B, 4_096);
    let before = h.position(Deck::B);

    // Deck A is handed something that cannot be decoded, and asked to play it.
    h.engine.load(Deck::A, &broken);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.events().iter().any(|e| e.contains("Error")) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.events().iter().any(|e| e.contains("Error")), "{:?}", h.events());
    h.engine.play(Deck::A);

    // Deck B carries on: the device is still running, the playhead is still
    // moving, and there is still audio coming out of it.
    let audio = h.play_until(Deck::B, before + 4_096);
    assert!(h.sink.running(), "the device stopped when the other deck failed");
    assert!(h.position(Deck::B) > before, "deck B's playhead stopped");
    let peak = audio.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    assert!(peak > 0.4, "deck B went quiet: {peak}");
}

#[test]
fn a_track_ending_on_one_deck_leaves_the_other_playing() {
    // The same property, for the ordinary way a deck runs out: a track that
    // ends is not an error, and the deck beside it must not notice.
    let dir = tempfile::tempdir().unwrap();
    let brief = dir.path().join("brief.wav");
    ramp(&brief, RATE as usize / 4);
    let long = dir.path().join("long.wav");
    ramp(&long, RATE as usize * 4);

    let h = harness();
    h.engine.load(Deck::A, &brief);
    h.wait_for_load(1);
    h.engine.load(Deck::B, &long);
    h.wait_for_load(2);
    h.engine.play(Deck::A);
    h.engine.play(Deck::B);

    // Past the end of the short one.
    h.play_until(Deck::B, u64::from(RATE) / 2);
    assert!(h.engine.snapshot().a.position_frames >= u64::from(RATE) / 4 - 4_096);

    let before = h.position(Deck::B);
    let audio = h.play_until(Deck::B, before + 8_192);
    assert!(h.sink.running(), "the device stopped when the short track ended");
    assert!(h.position(Deck::B) > before, "deck B's playhead stopped");
    let peak = audio.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    assert!(peak > 0.4, "deck B went quiet: {peak}");
}

#[test]
fn unloading_clears_the_deck() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ramp.wav");
    ramp(&path, RATE as usize);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    h.play_until(Deck::A, 1_024);

    h.engine.unload(Deck::A);
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.engine.snapshot().a.loaded && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let snapshot = h.engine.snapshot();
    assert!(!snapshot.a.loaded);
    assert!(!snapshot.a.playing);
    assert_eq!(snapshot.a.position_frames, 0);
}

#[test]
fn a_seek_is_honoured_at_once_even_while_the_decoder_runs_flat_out() {
    // A minute, so the far end is far past anything already decoded, and a
    // deck that decoded its way there would take a very long time about it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    // At 48 kHz against a 44.1 kHz device, so the resampler runs: that is what
    // most of the library will do on a real machine, and a deck that decodes
    // faster than anything can drain it never starves its own commands.
    ramp_at(&path, 48_000, 48_000 * 60);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    // Straight into playing and seeking, with nothing waited for in between:
    // the decode thread is filling from the start of the track at the moment
    // the seek arrives, which is when it used to ignore it.
    h.engine.play(Deck::A);

    // Fifty seconds into a sixty-second ramp is 0.917; the start is 0.5.
    let target = u64::from(RATE) * 50;
    h.engine.seek_frames(Deck::A, target);

    // Frames of audio from the *old* position that get played before the new
    // position arrives. Silence is not counted: an underrun while the decode
    // thread refills is a different thing from playing the wrong music, and
    // counting pulls rather than audio would just measure how busy the machine
    // is.
    let mut stale = 0_u64;
    let mut arrived = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !arrived && Instant::now() < deadline {
        // A whole ring at a time, which is what makes this a test of the
        // starvation: a consumer that takes less than the decode thread
        // produces lets the ring fill, and a full ring is what used to be the
        // only thing that sent the thread back to its channel.
        let audio = h.sink.pull(RING);
        arrived = audio.iter().any(|sample| *sample > 0.9);
        stale += audio.iter().filter(|sample| **sample > 0.4 && **sample < 0.9).count() as u64 / 2;
    }
    assert!(arrived, "audio from the new position never arrived");
    // A ring is 8,192 frames, and the decode thread checks its channel once a
    // ring, so two is the most that can already be in flight. Anything near a
    // second means the deck decoded its way to the seek point instead of
    // jumping — which is what happened while the decode loop could starve its
    // own command channel.
    assert!(stale < u64::from(RATE) / 2, "played {stale} frames from the old position");
}

#[test]
fn starting_opens_from_silence_rather_than_stepping_into_the_music() {
    // Press play on a loud passage and the first sample handed to the device
    // is whatever the waveform was doing. Straight from silence, that is a
    // click; this is the fade that makes it a start.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat(&path, RATE as usize);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);

    // The silence the device was putting out before play was pressed. Without
    // it the measurement starts at the first sample of music and has nothing
    // to compare it to, which is the one thing this test is about.
    let mut audio = h.sink.pull(64);
    assert!(audio.iter().all(|s| *s == 0.0), "a stopped deck must be silent");
    h.engine.play(Deck::A);
    audio.extend(h.play_until(Deck::A, 4_096));

    let peak = audio.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    assert!(peak > FLAT - 0.05, "the deck never reached full level: {peak}");
    let worst = worst_step(&audio);
    assert!(worst <= step_limit(), "starting stepped by {worst}");
}

#[test]
fn stopping_closes_to_silence_rather_than_cutting_the_waveform() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat(&path, RATE as usize * 2);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    let mut audio = h.play_until(Deck::A, 8_192);

    h.engine.pause(Deck::A);
    audio.extend(h.sink.pull(2_048));

    let worst = worst_step(&audio);
    assert!(worst <= step_limit(), "stopping stepped by {worst}");
    // And it ends at zero, which is the only place silence can start from.
    assert_eq!(audio.last().copied(), Some(0.0));
}

#[test]
fn a_seek_neither_cuts_what_was_playing_nor_opens_on_the_new_position() {
    // Cueing, jumping a beat and dragging the overview are all this seek. Both
    // ends of it are a step in the waveform without the fade: the old position
    // is cut wherever it was, and the new one opens wherever it starts.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat(&path, RATE as usize * 8);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    let mut audio = h.play_until(Deck::A, 8_192);

    // Far past anything the ring holds, so the deck really does jump.
    h.engine.seek_frames(Deck::A, u64::from(RATE) * 5);
    let landed = h.play_until(Deck::A, u64::from(RATE) * 5 + 8_192);
    audio.extend(landed);

    let worst = worst_step(&audio);
    assert!(worst <= step_limit(), "the seek stepped by {worst}");
    // The music did come back, rather than the deck simply going quiet.
    let peak = audio
        .chunks_exact(2)
        .skip(8_192)
        .map(|frame| frame[0].abs())
        .fold(0.0_f32, f32::max);
    assert!(peak > FLAT - 0.05, "nothing was heard after the seek: {peak}");
}

#[test]
fn several_seeks_in_a_row_still_do_not_click() {
    // A beat jump held down is a seek every few milliseconds, arriving while
    // the last one is still fading. Each one has to wait its turn rather than
    // cutting the fade in front of it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flat.wav");
    flat(&path, RATE as usize * 8);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);
    let mut audio = h.play_until(Deck::A, 4_096);

    for beat in 1..=6_u64 {
        h.engine.seek_frames(Deck::A, u64::from(RATE) * beat / 2);
        audio.extend(h.sink.pull(256));
    }
    audio.extend(h.play_until(Deck::A, u64::from(RATE) * 3 + 4_096));

    let worst = worst_step(&audio);
    assert!(worst <= step_limit(), "a run of seeks stepped by {worst}");
}

#[test]
fn the_end_of_a_track_fades_rather_than_being_cut_off() {
    // The last sample of a file is wherever the music was, and the silence
    // after it is a step down from there.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.wav");
    flat(&path, 8_000);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);
    h.engine.play(Deck::A);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut audio = Vec::new();
    while h.engine.snapshot().a.playing && Instant::now() < deadline {
        audio.extend(h.sink.pull(512));
    }
    assert!(!h.engine.snapshot().a.playing, "the deck never stopped at the end");
    let worst = worst_step(&audio);
    assert!(worst <= step_limit(), "the end of the track stepped by {worst}");
}

/// A 220 Hz sine, which steps between neighbouring samples all by itself.
fn tone(path: &Path, frames: usize) {
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let value = 0.8 * (t * 220.0 * std::f32::consts::TAU).sin();
        samples.push(value);
        samples.push(value);
    }
    write_wav(path, RATE, 2, &samples);
}

/// How far a step has to stand out from its neighbours to be a click.
///
/// An absolute threshold says nothing about music: a loud high note steps by
/// most of full scale between samples all by itself. A click is a step that
/// does not belong to what is around it. The same measure `scrubcheck` uses.
const CLICK_RATIO: f32 = 12.0;

/// Samples either side a step is compared against: about two milliseconds.
const NEIGHBOURHOOD: usize = 96;

/// Steps that stand out from the music around them.
fn clicks(out: &[f32]) -> usize {
    let frames: Vec<f32> = out.chunks_exact(2).map(|frame| frame[0]).collect();
    let steps: Vec<f32> = frames.windows(2).map(|pair| (pair[1] - pair[0]).abs()).collect();
    let mut found = 0;
    for i in NEIGHBOURHOOD..steps.len().saturating_sub(NEIGHBOURHOOD) {
        let around: f32 = steps[i - NEIGHBOURHOOD..i]
            .iter()
            .chain(&steps[i + 1..i + 1 + NEIGHBOURHOOD])
            .sum::<f32>()
            / (NEIGHBOURHOOD * 2) as f32;
        // A silent neighbourhood has nothing to stand out from.
        if around > 1e-4 && steps[i] > around * CLICK_RATIO {
            found += 1;
        }
    }
    found
}

#[test]
fn no_transport_move_stands_out_from_the_music_around_it() {
    // The flat-track tests measure the step exactly; this one asks the
    // question the way an ear does, on a signal that is stepping anyway.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    tone(&path, RATE as usize * 8);

    let h = harness();
    h.engine.load(Deck::A, &path);
    h.wait_for_load(1);

    let mut audio = h.sink.pull(64);
    h.engine.play(Deck::A);
    audio.extend(h.play_until(Deck::A, 8_192));
    // A cue, then a beat jump, then the overview dropped somewhere else.
    for at in [u64::from(RATE) * 4, u64::from(RATE) * 2, u64::from(RATE) * 6] {
        h.engine.seek_frames(Deck::A, at);
        audio.extend(h.play_until(Deck::A, at + 8_192));
    }
    h.engine.pause(Deck::A);
    audio.extend(h.sink.pull(2_048));

    let found = clicks(&audio);
    assert_eq!(found, 0, "{found} steps stood out from the music");
    assert_eq!(audio.last().copied(), Some(0.0), "the stream must end at silence");
}
