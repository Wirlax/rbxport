//! The playback engine, and the tick the interface reads it through.
//!
//! The engine itself is `rbl-deck` and knows nothing about Tauri. This is the
//! adapter: it opens the audio device the first time a deck is given something
//! to do, rather than at launch. A library manager that holds an output device
//! for a deck nobody has touched is a library manager that shows up in the
//! system's audio list for no reason; the stream is paused again as soon as
//! neither deck is playing.
//!
//! Position does not come through a command. One event, ten times a second,
//! carries both decks, and the interface extrapolates between ticks from the
//! frame counter. Sixty ticks a second per deck would be pure IPC churn, and
//! the interface would still have to interpolate to draw a smooth playhead.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rbl_deck::{Deck, DeckEvent, Engine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult, ErrorKind};

/// How often a tick goes out while something is playing.
const TICK: Duration = Duration::from_millis(100);

/// One deck in a tick.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckTickDto {
    pub frames: u64,
    pub total_frames: u64,
    /// Bumped on every load and seek, so the interface snaps its playhead
    /// rather than easing it towards a position it did not expect.
    pub generation: u32,
    pub playing: bool,
    pub loaded: bool,
}

/// Both decks, which is what one tick carries: about 200 bytes, well inside
/// the 1 KB event cap.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickDto {
    pub a: DeckTickDto,
    pub b: DeckTickDto,
    pub sample_rate: u32,
}

impl TickDto {
    /// What to report before the engine exists: two empty decks. A deck that
    /// has never been opened is stopped at zero, which is the truth.
    pub fn silent() -> Self {
        let empty = DeckTickDto {
            frames: 0,
            total_frames: 0,
            generation: 0,
            playing: false,
            loaded: false,
        };
        Self { a: empty, b: empty, sample_rate: 0 }
    }
}

/// What a deck reports outside the tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckEventDto {
    pub deck: String,
    pub total_frames: u64,
    pub sample_rate: u32,
    pub message: Option<String>,
}

/// Holds the engine, which is not built until something is played.
#[derive(Default)]
pub struct Player {
    engine: Mutex<Option<Arc<Engine>>>,
    /// Whether a ticker is already running, so play does not start a second.
    ticking: std::sync::atomic::AtomicBool,
}

impl Player {
    /// The engine, opening the audio device on first use.
    ///
    /// Opening it is what makes noise possible, so it happens when a deck is
    /// asked to do something and not before. The stream itself stays paused
    /// until something plays.
    pub fn engine(&self, app: &AppHandle) -> AppResult<Arc<Engine>> {
        let mut held = self.engine.lock();
        if let Some(engine) = held.as_ref() {
            return Ok(Arc::clone(engine));
        }
        let handle = app.clone();
        let events: rbl_deck::EventSink = Arc::new(move |event: DeckEvent| {
            emit_deck_event(&handle, &event);
        });
        let engine = Engine::new(&events).map_err(|e| {
            AppError::new(ErrorKind::Internal, "The audio device could not be opened.")
                .with_detail(e.to_string())
        })?;
        let engine = Arc::new(engine);
        *held = Some(Arc::clone(&engine));
        Ok(engine)
    }

    /// Already-built engine only — for a tick, which must not open a device.
    pub fn opened(&self) -> Option<Arc<Engine>> {
        self.engine.lock().clone()
    }

    pub fn ticking(&self) -> &std::sync::atomic::AtomicBool {
        &self.ticking
    }
}

fn emit_deck_event(app: &AppHandle, event: &DeckEvent) {
    let (name, payload) = match event {
        DeckEvent::Loaded { deck, total_frames, sample_rate } => (
            "deck:loaded",
            DeckEventDto {
                deck: deck.name().to_owned(),
                total_frames: *total_frames,
                sample_rate: *sample_rate,
                message: None,
            },
        ),
        DeckEvent::Error { deck, message } => (
            "deck:error",
            DeckEventDto {
                deck: deck.name().to_owned(),
                total_frames: 0,
                sample_rate: 0,
                message: Some(message.clone()),
            },
        ),
    };
    if let Err(e) = app.emit(name, payload) {
        tracing::warn!(error = %e, "a deck event did not reach the interface");
    }
}

/// Starts the ticker if one is not already running.
///
/// It stops as soon as neither deck is playing, which is what keeps an idle
/// window at no measurable cost. Every play starts it again.
pub fn start_ticker(app: &AppHandle) {
    let player = app.state::<Arc<Player>>();
    if player.ticking().swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    // Its own thread rather than the async runtime: this is a fixed 10 Hz beat
    // that ends when playback does, and it must not share a runtime worker
    // with a command that is reading the database.
    let spawned = std::thread::Builder::new().name("rbl-deck-tick".to_owned()).spawn(move || {
        loop {
            std::thread::sleep(TICK);
            let player = handle.state::<Arc<Player>>();
            let Some(engine) = player.opened() else { break };
            let snapshot = engine.snapshot();
            if let Err(e) = handle.emit("deck:tick", tick_of(&snapshot)) {
                tracing::warn!(error = %e, "a deck tick did not reach the interface");
            }
            if !snapshot.any_playing() {
                break;
            }
        }
        let player = handle.state::<Arc<Player>>();
        player.ticking().store(false, std::sync::atomic::Ordering::SeqCst);
    });
    if let Err(e) = spawned {
        tracing::error!(error = %e, "the deck ticker could not be started");
        player.ticking().store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn tick_of(snapshot: &rbl_deck::Snapshot) -> TickDto {
    let deck = |s: &rbl_deck::DeckSnapshot| DeckTickDto {
        frames: s.position_frames,
        total_frames: s.total_frames,
        generation: s.generation,
        playing: s.playing,
        loaded: s.loaded,
    };
    TickDto { a: deck(&snapshot.a), b: deck(&snapshot.b), sample_rate: snapshot.sample_rate }
}

/// Which deck a command names. Unknown names are deck A rather than an error:
/// the interface only ever sends what it was given in a tick.
pub fn deck_of(name: &str) -> Deck {
    match name {
        "b" | "B" => Deck::B,
        _ => Deck::A,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_deck_name_maps_to_a_deck_and_never_fails() {
        assert_eq!(deck_of("a"), Deck::A);
        assert_eq!(deck_of("b"), Deck::B);
        assert_eq!(deck_of("B"), Deck::B);
        // The interface only ever sends what a tick gave it, so an unknown
        // name is a bug elsewhere rather than something to refuse a command
        // over; it plays on deck A.
        assert_eq!(deck_of("nonsense"), Deck::A);
    }

    #[test]
    fn a_tick_before_the_engine_exists_is_two_stopped_decks() {
        let tick = TickDto::silent();
        assert_eq!(tick.a.frames, 0);
        assert!(!tick.a.playing);
        assert!(!tick.b.loaded);
        assert_eq!(tick.sample_rate, 0);
    }

    #[test]
    fn a_tick_is_small_enough_for_the_event_cap() {
        // The cap on an event payload is 1 KB; this carries both decks.
        let json = serde_json::to_string(&TickDto::silent()).unwrap();
        assert!(json.len() < 1_024, "a tick was {} bytes", json.len());
    }
}
