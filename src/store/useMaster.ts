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
 * How much of the last reading a meter keeps when the next is quieter.
 *
 * Fast attack, slow release, as every meter has: 0.8 a frame at thirty a
 * second falls about 30 dB a second, which reads as a needle settling rather
 * than as a bar flickering.
 */
const RELEASE = 0.8;

/**
 * The value a meter shows next: the reading, or the last one decayed.
 *
 * Fast attack, slow release. A meter that simply took each reading flickers,
 * and the loud moment — the one worth seeing — is gone before the eye is.
 */
export function nextPeak(shown: number, reading: number): number {
  const safe = Number.isFinite(reading) ? Math.max(reading, 0) : 0;
  const held = Number.isFinite(shown) ? Math.max(shown, 0) * RELEASE : 0;
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
      const unlisten = backend.onMeters((meters) => {
        if (!live) return;
        setState((current) => {
          // A peak falls back rather than dropping: a meter that snaps to the
          // next reading flickers, and the loud moment is the one to see.
          const left = nextPeak(current.peakLeft, meters.peakLeft);
          const right = nextPeak(current.peakRight, meters.peakRight);
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
