/**
 * The master level and its meters, for the top bar.
 *
 * Read from the same tick the decks are: the engine emits one, ten times a
 * second, and this picks the three fields out of it rather than asking for
 * them separately. The level is written back through a command, and comes
 * home on the next tick — what the callback actually applied, not what it was
 * asked for.
 */
import { useCallback, useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";

/**
 * How fast a meter falls, in decibels a second.
 *
 * A peak programme meter's own figure: 20 dB in a second reads as a needle
 * settling. Per frame it was too fast to see — 20 % of what was left every
 * thirtieth of a second is a bar that has gone before the eye arrives.
 */
const FALL_DB_PER_SECOND = 20;

/** The longest step the fall is applied over, so a stall is not a jump to nil. */
const MAX_STEP_SECONDS = 0.25;

/**
 * The value a meter shows next: the reading, or the last one fallen.
 *
 * Fast attack, slow release. A meter that simply took each reading flickers,
 * and the loud moment — the one worth seeing — is gone before the eye is.
 *
 * The fall is measured in time rather than in frames, so it looks the same
 * whatever rate the meters arrive at and does not race if a frame is dropped.
 */
export function nextPeak(shown: number, reading: number, seconds: number): number {
  const safe = Number.isFinite(reading) ? Math.max(reading, 0) : 0;
  const previous = Number.isFinite(shown) ? Math.max(shown, 0) : 0;
  const step = Number.isFinite(seconds) ? Math.min(Math.max(seconds, 0), MAX_STEP_SECONDS) : 0;
  const held = previous * 10 ** ((-FALL_DB_PER_SECOND * step) / 20);
  return Math.min(Math.max(safe, held), 1);
}

export interface Master {
  /** 0 to 1. */
  level: number;
  peakLeft: number;
  peakRight: number;
  setLevel: (level: number) => void;
}

export function useMaster(): Master {
  const [state, setState] = useState({ level: 1, peakLeft: 0, peakRight: 0 });

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      // The meters come on their own beat, three times as often as the decks.
      let last = performance.now();
      const unlisten = backend.onMeters((meters) => {
        if (!live) return;
        const now = performance.now();
        const elapsed = (now - last) / 1000;
        last = now;
        setState((current) => {
          // A peak falls back rather than dropping: a meter that snaps to the
          // next reading flickers, and the loud moment is the one to see.
          const left = nextPeak(current.peakLeft, meters.peakLeft, elapsed);
          const right = nextPeak(current.peakRight, meters.peakRight, elapsed);
          return current.level === meters.master &&
            current.peakLeft === left &&
            current.peakRight === right
            ? current
            : { level: meters.master, peakLeft: left, peakRight: right };
        });
      });
      if (!live) {
        unlisten();
        return;
      }
      stop = unlisten;
      // What it holds now, so a reload does not show the knob at the top.
      const now = await backend.deckState();
      if (live) {
        setState({ level: now.master, peakLeft: now.peakLeft, peakRight: now.peakRight });
      }
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const setLevel = useCallback((level: number) => {
    // Locally first: a knob that waits a tenth of a second to move is a knob
    // that feels broken.
    setState((current) => ({ ...current, level }));
    void (async () => {
      try {
        const backend = await getBackend();
        await backend.setMasterLevel(level);
      } catch {
        // No engine behind this build; the knob still turns.
      }
    })();
  }, []);

  return { ...state, setLevel };
}
