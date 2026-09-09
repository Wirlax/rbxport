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
      const unlisten = backend.onDeckTick((tick) => {
        if (!live) return;
        setState((current) =>
          current.level === tick.master &&
          current.peakLeft === tick.peakLeft &&
          current.peakRight === tick.peakRight
            ? current
            : { level: tick.master, peakLeft: tick.peakLeft, peakRight: tick.peakRight },
        );
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
