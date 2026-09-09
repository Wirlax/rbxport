//! What a fast drag sounds like: pulls the sink in real time while the pointer
//! is dragged, and reports how long the audio ran after the hand stopped.
//!
//! A drag that grinds on for seconds after the pointer stops is the bug this
//! guards. READ-ONLY.
//!
//! `cargo run --release -p rbl-deck --example scrubcheck -- <audio file>`
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const BUFFER: usize = 512;

fn main() {
    let Some(path) = std::env::args().nth(1).map(std::path::PathBuf::from) else {
        println!("usage: scrubcheck <audio file>");
        return;
    };
    let (tx, rx) = mpsc::channel();
    let events: rbl_deck::EventSink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let (sink_tx, sink_rx) = mpsc::channel();
    let engine = rbl_deck::Engine::with_sink(
        move |render| {
            let sink = Arc::new(rbl_deck::NullSink::new(44_100, render));
            let _ = sink_tx.send(Arc::clone(&sink));
            Ok(sink as Arc<dyn rbl_deck::Sink>)
        },
        &events,
    )
    .expect("engine");
    let sink = sink_rx.recv().expect("sink");
    engine.load(rbl_deck::Deck::A, &path);
    let Ok(rbl_deck::DeckEvent::Loaded { sample_rate, .. }) =
        rx.recv_timeout(Duration::from_secs(20))
    else {
        println!("did not load");
        return;
    };

    let period = Duration::from_secs_f64(BUFFER as f64 / f64::from(sample_rate));
    engine.seek_ms(rbl_deck::Deck::A, 60_000);
    std::thread::sleep(Duration::from_millis(200));
    engine.scrub_begin(rbl_deck::Deck::A);

    // A slow drag first: it should be audible throughout, because the head
    // keeps up and the lag cap never comes into it.
    let slow_start = Instant::now();
    let mut slow_at = 60_000_u64;
    let (mut slow_sounding, mut slow_pulls) = (0_u32, 0_u32);
    while slow_start.elapsed() < Duration::from_millis(400) {
        slow_at += 20;
        engine.scrub_to_ms(rbl_deck::Deck::A, slow_at);
        if sink.pull(BUFFER).iter().any(|s| *s != 0.0) {
            slow_sounding += 1;
        }
        slow_pulls += 1;
        std::thread::sleep(period);
    }
    println!("a slow drag: audible in {slow_sounding} of {slow_pulls} buffers");

    // A flick: the pointer crosses thirty seconds of music in a fifth of a
    // second, then stops dead.
    let start = Instant::now();
    let mut at = slow_at;
    let mut sounding_while_dragging = 0_u32;
    while start.elapsed() < Duration::from_millis(200) {
        at += 1_500;
        engine.scrub_to_ms(rbl_deck::Deck::A, at);
        if sink.pull(BUFFER).iter().any(|s| *s != 0.0) {
            sounding_while_dragging += 1;
        }
        std::thread::sleep(period);
    }

    // The hand has stopped. How long does the audio keep going?
    let stopped = Instant::now();
    let mut last_sound = stopped;
    while stopped.elapsed() < Duration::from_secs(3) {
        if sink.pull(BUFFER).iter().any(|s| *s != 0.0) {
            last_sound = Instant::now();
        }
        std::thread::sleep(period);
    }
    engine.scrub_end(rbl_deck::Deck::A);
    std::thread::sleep(Duration::from_millis(100));

    println!("a flick: {} ms of music in 200 ms of wall time", at - slow_at);
    println!("  audible in {sounding_while_dragging} of the drag's buffers");
    println!("  kept sounding for {:?} after the pointer stopped", last_sound - stopped);
    println!(
        "  landed at {} ms, pointer was at {at} ms",
        engine.snapshot().a.position_frames * 1000 / u64::from(sample_rate.max(1)),
    );
}
