/**
 * A button that repeats while held, the way rekordbox's grid shift and
 * widen/narrow buttons do: once on the press, then again and again after a
 * short delay until the pointer lets go or leaves.
 *
 * Pointer events, not `mousedown`, so a touch holds too; a click that came
 * from the keyboard (its `detail` is 0) fires once through `onClick`, which
 * a pointer's own click is told to ignore because the press already fired.
 * The repeats are timeouts, one at a time, never an interval: nothing runs
 * once the button is let go.
 */
import { useCallback, useEffect, useRef } from "react";

/** Before the first repeat, and between repeats after it. */
export const HOLD_DELAY_MS = 1000;
export const HOLD_REPEAT_MS = 100;

export interface HoldHandlers {
  onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onPointerUp: () => void;
  onPointerLeave: () => void;
  onPointerCancel: () => void;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
}

export function useHoldRepeat(): (action: (held?: boolean) => void) => HoldHandlers {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const stop = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  }, []);
  useEffect(() => stop, [stop]);

  return useCallback(
    (action: (held?: boolean) => void): HoldHandlers => {
      const tick = () => {
        action(true);
        timer.current = setTimeout(tick, HOLD_REPEAT_MS);
      };
      return {
        onPointerDown: (event) => {
          if (event.button !== 0) return;
          stop();
          action();
          timer.current = setTimeout(tick, HOLD_DELAY_MS);
        },
        onPointerUp: stop,
        onPointerLeave: stop,
        onPointerCancel: stop,
        onClick: (event) => {
          if (event.detail === 0) action();
        },
      };
    },
    [stop],
  );
}
