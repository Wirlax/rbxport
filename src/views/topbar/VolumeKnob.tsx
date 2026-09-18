/**
 * The master level, drawn as rekordbox draws it: an arc that fills clockwise
 * from the bottom left, with a white pointer at the value.
 *
 * Dragged rather than clicked — a knob is a knob — and vertical, which is what
 * every mixer does: sideways on a control 18pt across is unusable.
 *
 * The scale is `volume.ts`'s: 0 at seven o'clock, 10 at five o'clock a
 * decibel under full, and past a detent 11 at six o'clock, full. While the
 * knob turns its reading is shown beside it. Reaching 10 the knob holds
 * there; keep pulling for a moment and it lets go to 11.
 */
import { useCallback, useRef, useState } from "react";

import { gainToKnob, KNOB_FULL, KNOB_TOP, knobLabel, knobToGain } from "@/lib/volume";
import styles from "./TopBar.module.css";

/** Degrees the travel sweeps, from seven o'clock to five. */
const SWEEP = 300;
/** Where that sweep starts, measured clockwise from twelve o'clock. */
const START = -150;
/** Where 11 sits: six o'clock. */
const FULL_ANGLE = 180;
/** Pixels of drag for the whole travel. A short throw is a twitchy knob. */
const THROW = 120;
/** How long the knob holds at 10 before a pull past it reaches 11. */
const DETENT_MS = 500;
/** How far past 10 the pointer has to go, once the hold is over. */
const DETENT_PX = 12;

export interface VolumeKnobProps {
  /** The engine's gain, 0 to 1. */
  level: number;
  onChange?: ((level: number) => void) | undefined;
}

/** The pointer's angle for a reading, clockwise from twelve. */
function angleOf(reading: number): number {
  if (reading >= KNOB_FULL) return FULL_ANGLE;
  return START + SWEEP * (Math.min(reading, KNOB_TOP) / KNOB_TOP);
}

/** A point on the ring, in the 24-unit box the SVG is drawn in. */
function point(angle: number, radius: number): [number, number] {
  const rad = ((angle - 90) * Math.PI) / 180;
  return [12 + Math.cos(rad) * radius, 12 + Math.sin(rad) * radius];
}

/** The `d` of an arc from one angle to another along the ring. */
function arc(from: number, to: number, radius: number): string {
  const [x1, y1] = point(from, radius);
  const [x2, y2] = point(to, radius);
  const large = to - from > 180 ? 1 : 0;
  return `M${x1.toFixed(2)} ${y1.toFixed(2)} A${radius} ${radius} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

export function VolumeKnob({ level, onChange }: VolumeKnobProps) {
  const reading = gainToKnob(Number.isFinite(level) ? level : 0);
  const grab = useRef<{ y: number; from: number; heldSince: number | null } | null>(null);
  const [turning, setTurning] = useState<number | null>(null);

  const move = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const held = grab.current;
      if (!held || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
      // Up is louder, which is why the drag is subtracted.
      const pulled = held.from + ((held.y - event.clientY) / THROW) * KNOB_TOP;
      let next: number;
      if (pulled < KNOB_TOP) {
        held.heldSince = null;
        next = Math.max(pulled, 0);
      } else {
        // The detent: 10 holds while the pull goes on, then lets go to 11.
        const now = performance.now();
        if (held.heldSince === null) held.heldSince = now;
        const past = ((pulled - KNOB_TOP) / KNOB_TOP) * THROW;
        next = now - held.heldSince >= DETENT_MS && past >= DETENT_PX ? KNOB_FULL : KNOB_TOP;
      }
      setTurning(next);
      onChange?.(knobToGain(next));
    },
    [onChange],
  );

  const shown = turning ?? reading;
  const angle = angleOf(shown);
  const [px, py] = point(angle, 7.2);
  return (
    <div
      className={styles.knob}
      role="slider"
      aria-label="Master level"
      aria-valuemin={0}
      aria-valuemax={KNOB_FULL}
      aria-valuenow={Math.round(shown * 10) / 10}
      aria-valuetext={knobLabel(shown)}
      tabIndex={0}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        grab.current = { y: event.clientY, from: reading, heldSince: null };
        setTurning(reading);
      }}
      onPointerMove={move}
      onPointerUp={(event) => {
        grab.current = null;
        setTurning(null);
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowUp") onChange?.(knobToGain(Math.min(Math.round(reading) + 1, KNOB_FULL)));
        if (event.key === "ArrowDown") onChange?.(knobToGain(Math.max(Math.round(reading) - 1, 0)));
      }}
    >
      <svg viewBox="0 0 24 24" aria-hidden focusable="false">
        <path className={styles.knobTrack} d={arc(START, START + SWEEP, 7.2)} />
        {shown > 0 ? <path className={styles.knobFill} d={arc(START, angle, 7.2)} /> : null}
        {/* The pointer, from the hub to the value. */}
        <line className={styles.knobPointer} x1="12" y1="12" x2={px.toFixed(2)} y2={py.toFixed(2)} />
      </svg>
      {turning !== null ? <span className={styles.knobValue}>{knobLabel(turning)}</span> : null}
    </div>
  );
}
