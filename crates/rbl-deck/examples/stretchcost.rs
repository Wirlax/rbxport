//! What two decks stretching at once cost, as a share of the time they have.
//!
//! The plan's first task for the tempo work: measure the CPU spike before
//! deciding anything else about it. Two `Wsola` stretchers, run over a minute
//! of audio each at a realistic tempo offset, timed against realtime.
//!
//! `cargo run --release -p rbl-deck --example stretchcost`
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use rbl_deck::{Stretcher, Wsola};

const RATE: u32 = 44_100;
const BUFFER: usize = 512;

fn main() {
    let seconds = 60_usize;
    // Something with content at every frequency, so the similarity search has
    // real work to do rather than locking onto one period.
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut noise = |n: usize| -> Vec<f32> {
        (0..n * 2)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 40) as f32 / 8_388_608.0 - 1.0
            })
            .collect()
    };

    for ratio in [1.0_f32, 1.06, 0.92] {
        let mut decks = [Wsola::new(RATE), Wsola::new(RATE)];
        for deck in &mut decks {
            deck.set_ratio(ratio);
        }
        let mut out = vec![0.0_f32; BUFFER * 2];
        let buffers = seconds * RATE as usize / BUFFER;
        let feed = noise(BUFFER * 2);
        let start = Instant::now();
        for _ in 0..buffers {
            for deck in &mut decks {
                while deck.wants() >= BUFFER {
                    deck.feed(&feed);
                }
                deck.pull(&mut out);
            }
        }
        let took = start.elapsed();
        println!(
            "two stretchers at {ratio:>5}x over {seconds} s: {:>6.1} ms, {:.3}% of realtime, \
             {:>5.1} us per {BUFFER}-frame buffer of the {:.0} it has",
            took.as_secs_f64() * 1000.0,
            took.as_secs_f64() / seconds as f64 * 100.0,
            took.as_secs_f64() * 1e6 / buffers as f64,
            BUFFER as f64 / f64::from(RATE) * 1e6,
        );
    }
}
