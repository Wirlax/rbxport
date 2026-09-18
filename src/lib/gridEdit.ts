/**
 * The GRID panel's arithmetic that lives on this side of the IPC.
 *
 * The grid itself is edited in Rust — `crates/rbl-anlz/src/grid.rs` is what
 * rewrites the track's analysis file — and the mock backend needs the same
 * edits so the panel can be driven end to end without a library. `applyEdit`
 * mirrors that module beat for beat and is tested against the same cases;
 * nothing in the views calls it. `tapTempo` and the button steps are the
 * panel's own.
 */
import type { GridEdit } from "@/ipc/types";

/** One beat as the mock keeps it: the three fields of a `PQTZ` entry. */
export interface EditableBeat {
  /** 1 to 4; 1 is the downbeat. */
  number: number;
  /** BPM x100 at this beat. */
  tempoX100: number;
  timeMs: number;
}

/**
 * How far the shift buttons move the grid: one millisecond, which is what
 * rekordbox's `Shift Beatgrid left/right` do per press [ASSUME: the manual
 * says "slightly"; a millisecond is the grid's own resolution].
 */
export const SHIFT_MS = 1;

/** How much the widen and narrow buttons change the tempo: 0.01 BPM. */
export const STRETCH_X100 = 1;

/**
 * Taps further apart than this start a new tap, not a slow tempo: two
 * seconds is 30 BPM, below anything a grid describes.
 */
export const TAP_GAP_MS = 2000;

/** Taps averaged, at most: the last eight, as a tap-tempo button does. */
export const TAP_WINDOW = 8;

/**
 * The tempo a run of taps describes, BPM x100, or `null` before the second
 * tap. The mean interval of the last `TAP_WINDOW` taps, so a stray early
 * one washes out as tapping goes on.
 */
export function tapTempo(tapsMs: readonly number[]): number | null {
  const recent = tapsMs.slice(-TAP_WINDOW);
  if (recent.length < 2) return null;
  const first = recent[0] ?? 0;
  const last = recent[recent.length - 1] ?? first;
  const interval = (last - first) / (recent.length - 1);
  if (interval <= 0) return null;
  return Math.round(6_000_000 / interval);
}

/**
 * The taps to keep after a new one: all of them while they come in time,
 * and the new one alone after a gap.
 */
export function withTap(tapsMs: readonly number[], atMs: number): number[] {
  const last = tapsMs[tapsMs.length - 1];
  if (last === undefined || atMs - last > TAP_GAP_MS || atMs < last) return [atMs];
  return [...tapsMs, atMs];
}

const MAX_BEATS = 1 << 20;

function barPosition(offset: number): number {
  return ((((offset % 4) + 4) % 4)) + 1;
}

function nearest(beats: readonly EditableBeat[], timeMs: number): number {
  let best = 0;
  let distance = Number.POSITIVE_INFINITY;
  beats.forEach((beat, i) => {
    const d = Math.abs(beat.timeMs - timeMs);
    if (d < distance) {
      distance = d;
      best = i;
    }
  });
  return best;
}

function firstDownbeat(beats: readonly EditableBeat[]): number {
  const at = beats.findIndex((beat) => beat.number === 1);
  return at < 0 ? 0 : at;
}

function renumberFrom(beats: readonly EditableBeat[], downbeat: number): EditableBeat[] {
  return beats.map((beat, i) => ({ ...beat, number: barPosition(i - downbeat) }));
}

function nudge(beats: readonly EditableBeat[], by: number): EditableBeat[] {
  const downbeat = firstDownbeat(beats);
  const moved = beats
    .map((beat) => ({ ...beat, timeMs: Math.max(beat.timeMs + by, 0) }))
    .filter((beat) => beat.timeMs > 0);
  if (moved.length === 0) return [];
  const dropped = beats.length - moved.length;
  return renumberFrom(moved, (((downbeat - dropped) % 4) + 4) % 4);
}

function double(beats: readonly EditableBeat[]): EditableBeat[] {
  const downbeat = firstDownbeat(beats);
  const out: EditableBeat[] = [];
  beats.forEach((beat, i) => {
    const tempoX100 = Math.min(beat.tempoX100 * 2, 65535);
    out.push({ ...beat, tempoX100 });
    const next = beats[i + 1];
    if (next) {
      out.push({ number: 1, tempoX100, timeMs: beat.timeMs + Math.floor(Math.max(next.timeMs - beat.timeMs, 0) / 2) });
    }
  });
  return renumberFrom(out, downbeat * 2);
}

function halve(beats: readonly EditableBeat[]): EditableBeat[] {
  const downbeat = firstDownbeat(beats);
  const phase = downbeat % 2;
  const out = beats
    .filter((_, i) => i % 2 === phase)
    .map((beat) => ({ ...beat, tempoX100: Math.floor(beat.tempoX100 / 2) }));
  if (out.length === 0) return [];
  return renumberFrom(out, Math.floor(downbeat / 2));
}

function retempo(beats: readonly EditableBeat[], bpmX100: number, anchorMs: number): EditableBeat[] {
  const first = beats[0];
  const last = beats[beats.length - 1];
  if (!first || !last) return [];
  if (bpmX100 <= 0) return beats.map((beat) => ({ ...beat }));
  const beatMs = 6_000_000 / bpmX100;
  const number = beats[nearest(beats, anchorMs)]?.number ?? 1;
  const lo = Math.min(first.timeMs, anchorMs) - beatMs / 2;
  const hi = Math.max(last.timeMs, anchorMs) + beatMs / 2;
  const bound = (edge: number) => Math.trunc(Math.max(-MAX_BEATS, Math.min(MAX_BEATS, (edge - anchorMs) / beatMs)));
  const out: EditableBeat[] = [];
  for (let k = bound(lo) - 1; k <= bound(hi) + 1; k++) {
    const time = anchorMs + k * beatMs;
    if (time < 1 || time <= lo || time >= hi) continue;
    out.push({ number: barPosition(number - 1 + k), tempoX100: bpmX100, timeMs: Math.round(time) });
    if (out.length >= MAX_BEATS) break;
  }
  return out;
}

/** The grid's tempo, from its first beat. */
export function tempoX100(beats: readonly EditableBeat[]): number {
  return beats[0]?.tempoX100 ?? 0;
}

/** Applies one edit to a whole grid, as `rbl_anlz::grid::apply` does. */
export function applyEdit(beats: readonly EditableBeat[], edit: GridEdit): EditableBeat[] {
  if (beats.length === 0) return [];
  switch (edit.kind) {
    case "nudge":
      return nudge(beats, edit.ms);
    case "double":
      return double(beats);
    case "halve":
      return halve(beats);
    case "downbeat":
      return renumberFrom(beats, nearest(beats, edit.timeMs));
    case "tempo":
      return retempo(beats, edit.bpmX100, edit.anchorMs);
    case "stretch": {
      const bpm = Math.max(1, Math.min(65535, tempoX100(beats) + edit.byX100));
      return retempo(beats, bpm, beats[0]?.timeMs ?? 0);
    }
    case "align": {
      const at = beats[nearest(beats, edit.timeMs)]?.timeMs ?? 0;
      return nudge(beats, edit.timeMs - at);
    }
    default:
      return beats.map((beat) => ({ ...beat }));
  }
}

/**
 * Applies an edit from the beat nearest `fromMs` on, the beats before it
 * left alone, as `rbl_anlz::grid::apply_from` does; `null` is the whole
 * grid.
 */
export function applyEditFrom(
  beats: readonly EditableBeat[],
  fromMs: number | null,
  edit: GridEdit,
): EditableBeat[] {
  if (fromMs === null) return applyEdit(beats, edit);
  if (beats.length === 0) return [];
  const at = nearest(beats, fromMs);
  const head = beats.slice(0, at).map((beat) => ({ ...beat }));
  const kept = head[head.length - 1]?.timeMs;
  const tail = applyEdit(beats.slice(at), edit).filter((beat) => kept === undefined || beat.timeMs > kept);
  return [...head, ...tail];
}
