/**
 * Playback for the preview player.
 *
 * The audio itself is in Rust — `crates/rbl-deck` — rather than on an
 * `<audio>` element. A media element has no primitive for phase-locked beat
 * sync, key sync or audible drag-scrub, which is what the 2-player view needs;
 * see `docs/player-engine.md`.
 *
 * Position does not come back from a command. The engine emits one tick ten
 * times a second carrying both decks' frame counters, and every frame in
 * between is that anchor plus the time since it arrived. Sixty ticks a second
 * would be IPC churn and the interface would still have to interpolate.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { AppErrorDto, DeckId, Tick } from "@/ipc/types";
import { canPlay } from "@/ipc/audio";
import { extrapolate, follow, NO_ANCHOR, type Anchor } from "@/lib/clock";

export interface Playback {
  /** True while audio is actually running. */
  playing: boolean;
  /**
   * Seconds elapsed, as React state — updated on each tick, ten times a
   * second, which is as fine as the readouts get and as often as the waveform
   * is worth redrawing.
   *
   * The playhead does not use this: see `subscribe`.
   */
  position: number;
  /** Seconds total, or 0 before the deck has said. */
  duration: number;
  /** Nothing to play: no track, or a build with no engine behind it. */
  idle: boolean;
  error: string | null;
  toggle: () => void;
  seek: (seconds: number) => void;
  /** Seek by fraction, for clicking the waveform. */
  seekFraction: (fraction: number) => void;
  /**
   * Dragging a waveform, with the audio following the pointer.
   *
   * `scrubBegin` starts the deck if it was stopped and remembers that it was,
   * `scrubTo` moves it, and `scrubEnd` puts the transport back the way it was
   * found. Measured on the reference library: seeking sixty times a second
   * while playing leaves 2 % of buffers empty and thirty times a second none
   * at all, so the audio follows a drag without a cache behind it.
   */
  scrubBegin: () => void;
  scrubTo: (seconds: number) => void;
  scrubEnd: () => void;
  /** Where playback is right now, without waiting for a render. */
  positionRef: React.RefObject<number>;
  /**
   * Every frame while playing, and once on each seek.
   *
   * The playhead is driven from here rather than from `position`: state feeds
   * the whole player subtree, so a faster tick only re-renders it faster. A
   * listener writes a transform straight to its own element instead.
   *
   * Returns its own unsubscribe.
   */
  subscribe: (listener: (seconds: number) => void) => () => void;
}

/** The preview player is deck A; the 2-player layout adds B. */
const DEFAULT_DECK: DeckId = "a";

/** What went wrong, in the words of whoever knows. */
const FALLBACK = "This track could not be played.";

/**
 * The reason, not a shrug.
 *
 * Every one of these failures arrives carrying why: a command rejects with an
 * `AppError` whose message says whether the file is missing or the audio
 * device would not open, and the deck's own error event carries what the
 * decoder said. Roughly one track in thirty of the reference library sits on a
 * volume that is not mounted, and "This track could not be played" leaves the
 * only useful fact — plug the drive in — on the floor.
 */
export function reasonFrom(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as Partial<AppErrorDto>).message;
    if (typeof message === "string" && message.trim() !== "") return message;
  }
  return FALLBACK;
}

export function usePlayback(trackId: string | null, DECK: DeckId = DEFAULT_DECK): Playback {
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // The live position, and who wants it every frame.
  const positionRef = useRef(0);
  const listeners = useRef(new Set<(seconds: number) => void>());
  // The last tick, which every frame in between is measured from.
  const anchor = useRef<Anchor>(NO_ANCHOR);
  /** The generation the playhead last snapped to. */
  const shownGeneration = useRef(0);
  /** Which track this deck was told to load, so a stale tick is ignored. */
  const loading = useRef<string | null>(null);
  /** Whether a drag is running, so a move is aimed rather than seeked. */
  const scrubbing = useRef(false);
  /** The seek a drag is waiting to send, coalesced to one a frame. */
  const pending = useRef<number | null>(null);
  const flushing = useRef(0);

  const emit = useCallback((seconds: number) => {
    positionRef.current = seconds;
    for (const listener of listeners.current) listener(seconds);
  }, []);

  const subscribe = useCallback((listener: (seconds: number) => void) => {
    listeners.current.add(listener);
    return () => {
      listeners.current.delete(listener);
    };
  }, []);

  /** Takes a tick as the truth about where the deck is. */
  const anchorOn = useCallback(
    (tick: Tick) => {
      const deck = DECK === "b" ? tick.b : tick.a;
      const rate = tick.sampleRate;
      anchor.current = {
        frames: deck.frames,
        at: performance.now(),
        sampleRate: rate,
        playing: deck.playing,
        generation: deck.generation,
        rate: 1,
      };
      setPlaying(deck.playing);
      setDuration(rate > 0 ? deck.totalFrames / rate : 0);
      const at = extrapolate(anchor.current, performance.now());
      setPosition(at);
      // A load or a seek moves the playhead deliberately; anything else is
      // drift, and is eased in rather than jumped.
      if (deck.generation !== shownGeneration.current) {
        shownGeneration.current = deck.generation;
        emit(at);
      }
    },
    [emit, DECK],
  );

  // The deck reports itself loaded, or says why it could not be.
  useEffect(() => {
    if (!canPlay) return;
    let live = true;
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      const unlistenTick = backend.onDeckTick((tick) => {
        if (live) anchorOn(tick);
      });
      const unlistenEvent = backend.onDeckEvent((event) => {
        if (!live || event.deck !== DECK) return;
        if (event.message !== null) {
          setError(event.message.trim() === "" ? FALLBACK : event.message);
          return;
        }
        setError(null);
        if (event.sampleRate > 0) setDuration(event.totalFrames / event.sampleRate);
      });
      if (!live) {
        unlistenTick();
        unlistenEvent();
        return;
      }
      stop = () => {
        unlistenTick();
        unlistenEvent();
      };
      // What the deck holds right now, so a reload does not start at zero.
      anchorOn(await backend.deckState());
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, [anchorOn, DECK]);

  // Point the deck at the selected track. Loading does not start playback:
  // choosing a track in the browser should not make noise.
  useEffect(() => {
    if (!canPlay) return;
    loading.current = trackId;
    anchor.current = NO_ANCHOR;
    setPosition(0);
    setDuration(0);
    setPlaying(false);
    setError(null);
    emit(0);
    void (async () => {
      try {
        const backend = await getBackend();
        if (loading.current !== trackId) return;
        if (trackId === null) await backend.deckUnload(DECK);
        else await backend.deckLoad(DECK, trackId);
      } catch (failure) {
        // A missing file, or no audio device at all. Either way the deck has
        // nothing, and which of the two it was is the whole of what the person
        // looking at it needs.
        if (loading.current === trackId) setError(reasonFrom(failure));
      }
    })();
  }, [trackId, emit, DECK]);

  // One frame loop for the whole player, running only while audio is, so an
  // idle window schedules nothing.
  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    let last = performance.now();
    const tick = () => {
      frame = requestAnimationFrame(tick);
      const now = performance.now();
      const target = extrapolate(anchor.current, now);
      const next = follow(positionRef.current, target, now - last);
      last = now;
      emit(next);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, emit]);

  const idle = !canPlay || trackId === null;

  const toggle = useCallback(() => {
    if (idle) return;
    const wanted = !playing;
    // The button follows at once rather than on the next tick, which is up to
    // a tenth of a second away.
    setPlaying(wanted);
    anchor.current = { ...anchor.current, playing: wanted, at: performance.now() };
    void (async () => {
      try {
        const backend = await getBackend();
        if (wanted) await backend.deckPlay(DECK);
        else await backend.deckPause(DECK);
      } catch (failure) {
        setPlaying(false);
        setError(reasonFrom(failure));
      }
    })();
  }, [idle, playing, DECK]);

  const seek = useCallback(
    (seconds: number) => {
      if (idle || !Number.isFinite(seconds)) return;
      const at = Math.max(0, seconds);
      // Locally first: the head must move under the pointer, not a tick later.
      anchor.current = {
        ...anchor.current,
        frames: anchor.current.sampleRate > 0 ? at * anchor.current.sampleRate : 0,
        at: performance.now(),
      };
      setPosition(at);
      emit(at);
      void (async () => {
        try {
          const backend = await getBackend();
          await backend.deckSeek(DECK, Math.round(at * 1000));
        } catch (failure) {
          setError(reasonFrom(failure));
        }
      })();
    },
    [idle, emit, DECK],
  );

  /**
   * Audio follows the pointer while a waveform is dragged.
   *
   * The engine does the work — see `crates/rbl-deck/src/scrub.rs`. It reads a
   * decoded window at the drag's own rate, so the pitch follows the hand and
   * pulling backwards plays backwards, which is what a record does. The
   * transport is not touched: a drag sounds whether or not the deck was
   * playing, and letting go leaves it as it was found.
   */
  const scrubBegin = useCallback(() => {
    if (idle || scrubbing.current) return;
    scrubbing.current = true;
    void (async () => {
      try {
        const backend = await getBackend();
        await backend.deckScrubBegin(DECK);
      } catch (failure) {
        setError(reasonFrom(failure));
      }
    })();
  }, [idle, DECK]);

  /**
   * Where the drag is now.
   *
   * The playhead and the waveform move on the spot; the message behind them is
   * coalesced to one a frame, because a trackpad emits pointer moves faster
   * than the screen refreshes and the engine only needs the latest.
   */
  const scrubTo = useCallback(
    (seconds: number) => {
      if (idle || !Number.isFinite(seconds)) return;
      const at = Math.max(seconds, 0);
      anchor.current = {
        ...anchor.current,
        frames: anchor.current.sampleRate > 0 ? at * anchor.current.sampleRate : 0,
        at: performance.now(),
      };
      setPosition(at);
      emit(at);
      pending.current = at;
      if (flushing.current) return;
      flushing.current = requestAnimationFrame(() => {
        flushing.current = 0;
        const target = pending.current;
        pending.current = null;
        if (target === null) return;
        void (async () => {
          try {
            const backend = await getBackend();
            if (scrubbing.current) await backend.deckScrubTo(DECK, Math.round(target * 1000));
            else await backend.deckSeek(DECK, Math.round(target * 1000));
          } catch (failure) {
            setError(reasonFrom(failure));
          }
        })();
      });
    },
    [idle, emit, DECK],
  );

  /** Lets go. The playhead stays where the drag left it. */
  const scrubEnd = useCallback(() => {
    if (!scrubbing.current) return;
    scrubbing.current = false;
    // Where the drag last aimed has to reach the deck before the drag ends,
    // because that is what the deck lands on. A click is over well inside one
    // frame, so the rAF that coalesces moves would still be holding the only
    // position anybody asked for when the end arrived, and the press would
    // land back where it started.
    if (flushing.current) {
      cancelAnimationFrame(flushing.current);
      flushing.current = 0;
    }
    const target = pending.current;
    pending.current = null;
    void (async () => {
      try {
        const backend = await getBackend();
        if (target !== null) await backend.deckScrubTo(DECK, Math.round(target * 1000));
        await backend.deckScrubEnd(DECK);
      } catch (failure) {
        setError(reasonFrom(failure));
      }
    })();
  }, [DECK]);

  // A drag that is still pending when the player goes away must not fire.
  useEffect(
    () => () => {
      if (flushing.current) cancelAnimationFrame(flushing.current);
    },
    [],
  );

  const seekFraction = useCallback(
    (fraction: number) => {
      if (duration <= 0) return;
      seek(Math.min(Math.max(fraction, 0), 1) * duration);
    },
    [duration, seek],
  );

  return {
    playing, position, duration, idle, error, toggle, seek, seekFraction,
    scrubBegin, scrubTo, scrubEnd, positionRef, subscribe,
  };
}
