/**
 * Drives the analysis queue against the backend.
 *
 * One track at a time: analysis decodes and runs a DSP pass, so several at
 * once would fight for the same cores and finish no sooner, while making the
 * progress meaningless.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { AnalysisResult } from "@/ipc/types";
import {
  cancel as cancelQueue,
  emptyQueue,
  enqueue,
  fail,
  isRunning,
  reset,
  start,
  succeed,
  total,
  type QueueItem,
  type QueueState,
} from "@/lib/queue";

export interface Analysis {
  state: QueueState;
  running: boolean;
  total: number;
  add: (items: readonly QueueItem[]) => void;
  cancel: () => void;
  clear: () => void;
}

export function useAnalysis(
  onAnalysed?: (trackId: string, result: AnalysisResult) => void,
  /** Called once when a run ends, whether it finished, failed or was stopped. */
  onDrained?: () => void,
): Analysis {
  const [state, setState] = useState<QueueState>(emptyQueue);
  // The effect below must not start a second track while one is in flight.
  const inFlight = useRef(false);
  // Whether a run has been going, so the drain fires once at its end.
  const ran = useRef(false);
  const running = isRunning(state);
  useEffect(() => {
    if (running) {
      ran.current = true;
    } else if (ran.current) {
      ran.current = false;
      onDrained?.();
    }
  }, [running, onDrained]);

  useEffect(() => {
    if (inFlight.current) return;
    if (state.current === null) {
      // Nothing running: take the next, if the run has not been cancelled.
      const next = start(state);
      if (next !== state) setState(next);
      return;
    }

    inFlight.current = true;
    const track = state.current;
    void (async () => {
      try {
        const backend = await getBackend();
        const result = await backend.analyseTrack(track.id);
        setState((s) => succeed(s));
        onAnalysed?.(track.id, result);
      } catch (e) {
        setState((s) => fail(s, e instanceof Error ? e.message : String(e)));
      } finally {
        inFlight.current = false;
      }
    })();
  }, [state, onAnalysed]);

  const add = useCallback((items: readonly QueueItem[]) => setState((s) => enqueue(s, items)), []);
  const cancel = useCallback(() => setState(cancelQueue), []);
  const clear = useCallback(() => setState(reset), []);

  // Memoised as a whole: see the note in `useColumns`.
  return useMemo(
    () => ({ state, running, total: total(state), add, cancel, clear }),
    [state, running, add, cancel, clear],
  );
}
