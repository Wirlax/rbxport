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

use rbl_deck::{Deck, DeckEvent, Engine, NullSink, Sink};

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
    // Pulling a stopped sink is silence, and the playhead stays where it was.
    assert!(h.sink.pull(1_024).iter().all(|s| *s == 0.0));
    assert_eq!(h.position(Deck::A), at);
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
