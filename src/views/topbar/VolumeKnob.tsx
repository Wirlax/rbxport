/**
 * The master level, drawn as rekordbox draws it: an arc that fills clockwise
 * from the bottom left, with a white pointer at the value.
 *
 * Dragged rather than clicked — a knob is a knob — and vertical, which is what
 * every mixer does: sideways on a control 18pt across is unusable.
 */
import { useCallback, useRef } from "react";

import styles from "./TopBar.module.css";

/** Degrees the arc sweeps, from its start at the bottom left. */
const SWEEP = 270;

/** Where that sweep starts, measured clockwise from twelve o'clock. */
const START = -135;

/** Pixels of drag for the whole range. A short throw is a twitchy knob. */
const THROW = 120;

export interface VolumeKnobProps {
  /** 0 to 1. */
  level: number;
  onChange?: ((level: number) => void) | undefined;
}

/** A point on the arc, in the 24-unit box the SVG is drawn in. */
function point(fraction: number, radius: number): [number, number] {
  const angle = ((START + SWEEP * fraction - 90) * Math.PI) / 180;
  return [12 + Math.cos(angle) * radius, 12 + Math.sin(angle) * radius];
}

/** The `d` of an arc from `from` to `to` along the ring. */
function arc(from: number, to: number, radius: number): string {
  const [x1, y1] = point(from, radius);
  const [x2, y2] = point(to, radius);
  const large = (to - from) * SWEEP > 180 ? 1 : 0;
  return `M${x1.toFixed(2)} ${y1.toFixed(2)} A${radius} ${radius} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

export function VolumeKnob({ level, onChange }: VolumeKnobProps) {
  const at = Math.min(Math.max(Number.isFinite(level) ? level : 0, 0), 1);
  const grab = useRef<{ y: number; from: number } | null>(null);

  const move = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const held = grab.current;
      if (!held || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
      // Up is louder, which is why the drag is subtracted.
      onChange?.(Math.min(Math.max(held.from + (held.y - event.clientY) / THROW, 0), 1));
    },
    [onChange],
  );

  const [px, py] = point(at, 7.2);
  return (
    <div
      className={styles.knob}
      role="slider"
      aria-label="Master level"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(at * 100)}
      tabIndex={0}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        grab.current = { y: event.clientY, from: at };
      }}
      onPointerMove={move}
      onPointerUp={(event) => {
        grab.current = null;
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowUp") onChange?.(Math.min(at + 0.05, 1));
        if (event.key === "ArrowDown") onChange?.(Math.max(at - 0.05, 0));
      }}
    >
      <svg viewBox="0 0 24 24" aria-hidden focusable="false">
        <path className={styles.knobTrack} d={arc(0, 1, 7.2)} />
        {at > 0 ? <path className={styles.knobFill} d={arc(0, at, 7.2)} /> : null}
        {/* The pointer, from the hub to the value. */}
        <line className={styles.knobPointer} x1="12" y1="12" x2={px.toFixed(2)} y2={py.toFixed(2)} />
      </svg>
    </div>
  );
}
