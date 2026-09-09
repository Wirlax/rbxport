/**
 * The player's arithmetic, kept out of the component so it can be tested.
 *
 * The detail waveform shows a fixed number of *bars*, not a fixed fraction of
 * the track. That is the difference between a zoom level and a scale: at a
 * fixed fraction a three-minute track and a ninety-minute mix show wildly
 * different amounts of music, and neither matches what a CDJ shows.
 */
import type { SortColumn } from "@/ipc/types";

/** Bars across the detail waveform. Matches rekordbox's default zoom. */
export const DETAIL_BARS = 12;

/** Beats in a bar. Everything here assumes 4/4, as rekordbox's grid does. */
export const BEATS_PER_BAR = 4;

/**
 * How much wider than the strip the scrolling layer is drawn.
 *
 * The detail waveform scrolls under a fixed playhead, and redrawing it on
 * every tick is what made it step ten times a second instead of moving. So it
 * is drawn once across twice the visible span and slid by a transform, which
 * the compositor does every frame for nothing; a redraw happens only when the
 * playhead has travelled far enough to see the end of what was drawn.
 */
export const OVERDRAW = 2;

/** Zoom levels, in bars across, that the +/- buttons and the wheel step through. */
export const ZOOM_STEPS = [2, 4, 8, 12, 16, 32, 64] as const;

/** Beat-jump sizes the size button cycles, as a CDJ offers them. */
export const JUMP_SIZES = [1, 2, 4, 8, 16, 32] as const;

/** The default, and what the button reads before anyone touches it. */
export const JUMP_BEATS = 4;

/** The next size along, wrapping — the button is a cycle, not a spinner. */
export function nextJumpSize(current: number): number {
  const at = JUMP_SIZES.indexOf(current as (typeof JUMP_SIZES)[number]);
  return JUMP_SIZES[(at + 1) % JUMP_SIZES.length] ?? JUMP_BEATS;
}

/** How long a jump of `beats` lasts at a tempo. Zero when there is no tempo. */
export function jumpSeconds(beats: number, bpmX100: number): number {
  if (bpmX100 <= 0 || beats <= 0) return 0;
  return (beats * 60) / (bpmX100 / 100);
}

/**
 * The zoom a wheel gesture lands on.
 *
 * Wheels differ wildly — a mouse notch is 100-odd pixels and a trackpad emits
 * a stream of ones — so this steps one level per call and lets the caller
 * decide what counts as a gesture. Up zooms in, which is the direction every
 * map and every waveform editor uses.
 */
export function zoomBy(bars: number, direction: number): number {
  const at = ZOOM_STEPS.indexOf(bars as (typeof ZOOM_STEPS)[number]);
  const from = at === -1 ? ZOOM_STEPS.indexOf(DETAIL_BARS) : at;
  const to = Math.min(Math.max(from + direction, 0), ZOOM_STEPS.length - 1);
  return ZOOM_STEPS[to] ?? DETAIL_BARS;
}

/**
 * What fraction of a track `bars` covers at a given tempo.
 *
 * Falls back to a fraction of the whole track when the tempo or length is
 * unknown, so an unanalysed track still draws something rather than dividing
 * by zero.
 */
export function detailSpan(bars: number, bpmX100: number, durationSec: number): number {
  if (bpmX100 <= 0 || durationSec <= 0) return 0.08;
  const bpm = bpmX100 / 100;
  const seconds = (bars * BEATS_PER_BAR * 60) / bpm;
  // Never more than the whole track, and never so small that a rounding error
  // collapses the window to nothing.
  return Math.min(1, Math.max(seconds / durationSec, 1e-4));
}

/**
 * The slice of the track a window of `span` centred on `at` covers.
 *
 * Not clamped to the track. The head stays in the middle and the waveform
 * moves under it, so at the start and the end the window hangs off the edge
 * and the empty half is drawn empty — which is what a CDJ shows, and the only
 * way the head can mean "here" rather than "somewhere in this strip".
 */
export function windowAround(at: number, span: number): { from: number; to: number } {
  const half = Math.max(span, 0) / 2;
  return { from: at - half, to: at + half };
}

/**
 * Where the playhead sits inside that window, as a percentage.
 *
 * Always the middle. The window moves instead — see `windowAround`.
 */
export function headPercent(): number {
  return 50;
}

/**
 * How far a drag of `dx` pixels moves the playhead, in seconds.
 *
 * Negated: dragging the waveform to the right pulls earlier music into view,
 * so the playhead goes back. The window's own span sets the rate, which is
 * what makes a zoomed-in drag fine and a zoomed-out one coarse rather than
 * both moving by the same amount of music per pixel.
 */
export function dragSeconds(
  dx: number,
  width: number,
  span: number,
  durationSec: number,
): number {
  if (width <= 0 || durationSec <= 0 || !Number.isFinite(dx)) return 0;
  return -(dx / width) * Math.max(span, 0) * durationSec;
}

/**
 * `-05:38.1` — time remaining, tenths in a smaller face.
 *
 * Minutes are padded to two digits, as rekordbox prints them everywhere in the
 * player: unpadded, the readout shifts by a character as a track crosses ten
 * minutes and the columns beside it move with it.
 *
 * Returned split so the caller can size the fraction differently.
 */
export function splitTime(seconds: number): { main: string; tenths: string } {
  const safe = Number.isFinite(seconds) ? Math.max(seconds, 0) : 0;
  const whole = Math.floor(safe);
  const tenths = Math.floor((safe - whole) * 10);
  const minutes = Math.floor(whole / 60);
  const rest = whole % 60;
  return {
    main: `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`,
    tenths: String(tenths),
  };
}

/**
 * `00:00:046` — how the memory list prints a position, to the millisecond.
 *
 * Built on `splitTime` so the minute padding is decided in one place; the two
 * lists disagreeing about that is exactly the sort of thing nobody notices
 * until the columns stop lining up.
 */
export function memoryTime(positionMs: number): string {
  const safe = Number.isFinite(positionMs) ? Math.max(positionMs, 0) : 0;
  const ms = Math.round(safe) % 1000;
  return `${splitTime(safe / 1000).main}:${String(ms).padStart(3, "0")}`;
}

/** Phrase kinds, as the colour tokens name them. */
export type PhraseKind =
  | "intro" | "verse" | "bridge" | "chorus"
  | "up" | "up2" | "up3" | "down" | "outro";

/**
 * Which colour token a phrase label takes.
 *
 * The labels come from the analysis, so this matches on what rekordbox writes
 * rather than on a numeric kind: `UP 1`, `UP 2` and `UP 3` are separate
 * colours, and everything else keys off its first word.
 */
export function phraseKind(label: string): PhraseKind {
  const text = label.trim().toUpperCase();
  if (text.startsWith("UP")) {
    if (text.includes("3")) return "up3";
    if (text.includes("2")) return "up2";
    return "up";
  }
  if (text.startsWith("DOWN")) return "down";
  if (text.startsWith("CHORUS")) return "chorus";
  if (text.startsWith("INTRO")) return "intro";
  if (text.startsWith("OUT")) return "outro";
  if (text.startsWith("VERSE")) return "verse";
  if (text.startsWith("BRIDGE")) return "bridge";
  // An unknown label still gets a block rather than a gap: a hole in the
  // phrase bar reads as missing analysis, which is a different problem.
  return "verse";
}

/** A phrase laid out as a fraction of the track. */
export interface PhraseSpan {
  label: string;
  kind: PhraseKind;
  from: number;
  to: number;
}

/**
 * Phrases as spans, each running to the start of the next.
 *
 * The analysis gives a start time per phrase and nothing else, so the end of
 * one is the start of the next and the last runs to the end of the track.
 */
export function phraseSpans(
  phrases: readonly { timeMs: number | null; beat: number; label: string }[],
  totalMs: number,
  /** Milliseconds a beat lasts, for phrases the beat grid did not reach. */
  beatMs = 0,
): PhraseSpan[] {
  if (totalMs <= 0) return [];
  // A phrase with no resolved time is placed from its beat where the tempo is
  // known, and dropped where it is not — a phrase at zero would stack every
  // unresolved block on the left edge and read as a mangled track.
  const at = (phrase: { timeMs: number | null; beat: number }): number | null =>
    phrase.timeMs ?? (beatMs > 0 ? (phrase.beat - 1) * beatMs : null);

  const placed = phrases
    .map((phrase) => ({ label: phrase.label, ms: at(phrase) }))
    .filter((phrase): phrase is { label: string; ms: number } => phrase.ms !== null)
    .sort((a, b) => a.ms - b.ms);

  return placed
    .map((phrase, i) => {
      const next = placed[i + 1];
      return {
        label: phrase.label,
        kind: phraseKind(phrase.label),
        from: Math.min(Math.max(phrase.ms / totalMs, 0), 1),
        to: Math.min(Math.max((next ? next.ms : totalMs) / totalMs, 0), 1),
      };
    })
    .filter((span) => span.to > span.from);
}

/** Column keys the player's readouts correspond to, for the info panel. */
export const READOUT_COLUMNS: readonly SortColumn[] = ["key", "bpm"];

/** Which set of controls the pad row is showing. */
export type PadMode = "cue" | "grid";

/** Which list the panel beside the deck is showing. */
export type CuePanel = "memory" | "hotCue" | "info";

/**
 * The cues one panel tab lists.
 *
 * Memory cues and hot cues are the same rows told apart by a flag, so the two
 * tabs are one filter rather than two fetches. Ordered by position, because a
 * cue list read out of order is unusable for finding a section.
 */
export function cuesFor<T extends { memory: boolean; positionMs: number }>(
  cues: readonly T[],
  panel: CuePanel,
): T[] {
  if (panel === "info") return [];
  const want = panel === "memory";
  return cues.filter((cue) => cue.memory === want).sort((a, b) => a.positionMs - b.positionMs);
}

/**
 * A track's beat grid, held as typed arrays rather than objects.
 *
 * Fetched whole, once per track: windowing it meant re-reading and re-parsing
 * the analysis file every time the playhead moved on. A three-hour mix is
 * about 23,000 beats, which is 23,000 small objects if this were a list and
 * 115 KB of typed array if it is not.
 */
export interface BeatGrid {
  /** Each beat's position in milliseconds, ascending. */
  times: Uint32Array;
  /** Each beat's number within its bar, 1 to 4. */
  numbers: Uint8Array;
}

/** Bytes one beat takes on the wire: a little-endian `u32`, then its number. */
const BEAT_BYTES = 5;

/** An empty grid, so a track without analysis is still a `BeatGrid`. */
export const NO_BEATS: BeatGrid = { times: new Uint32Array(), numbers: new Uint8Array() };

/**
 * Reads the backend's beat bytes.
 *
 * A trailing partial record is dropped rather than read past the end: the
 * backend never writes one, and a truncated read should draw fewer beats
 * rather than a beat at a garbage position.
 */
export function parseBeatGrid(bytes: Uint8Array): BeatGrid {
  const count = Math.floor(bytes.length / BEAT_BYTES);
  if (count === 0) return NO_BEATS;
  const times = new Uint32Array(count);
  const numbers = new Uint8Array(count);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < count; i++) {
    times[i] = view.getUint32(i * BEAT_BYTES, true);
    numbers[i] = view.getUint8(i * BEAT_BYTES + 4);
  }
  return { times, numbers };
}

/**
 * The most beats one window draws.
 *
 * The widest zoom is 64 bars, so 256 beats; this is generous for that and
 * stops a nonsense window from asking for thousands of spans.
 */
const MAX_DRAWN = 1024;

/** The index of the first beat at or after `ms`. */
function lowerBound(times: Uint32Array, ms: number): number {
  let lo = 0;
  let hi = times.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((times[mid] ?? 0) < ms) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/**
 * The beats inside a window, as the grid component draws them.
 *
 * A binary search rather than a filter over the whole grid: this runs on every
 * tick of the playhead, and scanning 23,000 beats to draw forty-eight of them
 * is the sort of thing that shows up as a dropped frame.
 */
export function beatsIn(
  grid: BeatGrid,
  fromMs: number,
  toMs: number,
): { timeMs: number; downbeat: boolean }[] {
  const out: { timeMs: number; downbeat: boolean }[] = [];
  if (toMs < fromMs) return out;
  for (let i = lowerBound(grid.times, fromMs); i < grid.times.length; i++) {
    const timeMs = grid.times[i] ?? 0;
    if (timeMs > toMs || out.length >= MAX_DRAWN) break;
    out.push({ timeMs, downbeat: grid.numbers[i] === 1 });
  }
  return out;
}

/**
 * The beat nearest `ms`, or `ms` itself when there is no grid.
 *
 * What the Q button does. A cue set by hand lands a few tens of milliseconds
 * off the beat and every loop and every mix from it inherits that; quantising
 * is why a CDJ's cues sit on the grid whatever the finger did.
 */
export function nearestBeatMs(grid: BeatGrid, ms: number): number {
  const { times } = grid;
  if (times.length === 0) return ms;
  const at = lowerBound(times, ms);
  const after = times[Math.min(at, times.length - 1)] ?? ms;
  // `lowerBound` gives the first beat at or after; the one before it is the
  // only other candidate, so this is two comparisons rather than a scan.
  const before = times[Math.max(at - 1, 0)] ?? after;
  return Math.abs(ms - before) <= Math.abs(after - ms) ? before : after;
}

/** What the deck should do, decided by the CUE button. */
export interface CueAction {
  /** Where to move the playhead, or `null` to leave it. */
  seekTo: number | null;
  /** Whether audio should be running after this. */
  playing: boolean;
  /** The cue point afterwards, which a press away from it moves. */
  cuePoint: number;
}

/** How close to the cue point still counts as being on it, in seconds. */
export const CUE_TOLERANCE = 0.02;

/**
 * Pressing CUE, as a CDJ does it.
 *
 * Three cases, and they are not variations of one thing:
 *
 * - **Playing.** Stop, and jump back to the cue point. This is the one people
 *   use mid-mix, and it is why the button is where it is.
 * - **Paused on the cue point.** Play for as long as the button is held, then
 *   snap back — `releaseCue` is the other half. Previewing the drop without
 *   losing your place is the whole point of the control.
 * - **Paused anywhere else.** Set the cue point here. A CDJ does not need a
 *   separate "set cue" button because this is it.
 */
export function pressCue(
  position: number,
  cuePoint: number,
  playing: boolean,
  /** The grid to snap a new cue point to, when Q is on. */
  quantiseTo: BeatGrid | null = null,
): CueAction {
  if (playing) return { seekTo: cuePoint, playing: false, cuePoint };
  if (Math.abs(position - cuePoint) <= CUE_TOLERANCE) {
    return { seekTo: null, playing: true, cuePoint };
  }
  const at = Math.max(position, 0);
  // With Q on the new cue lands on the nearest beat, and the playhead goes
  // there with it — a cue point the head is not standing on would immediately
  // read as "paused somewhere else" and the next press would move it again.
  const set = quantiseTo ? Math.max(nearestBeatMs(quantiseTo, at * 1000) / 1000, 0) : at;
  return { seekTo: set === at ? null : set, playing: false, cuePoint: set };
}

/**
 * Letting CUE go.
 *
 * Only a held preview does anything: the button was pressed on the cue point
 * and audio has been running since, so releasing it stops and rewinds. A
 * release that follows any other press is not a rewind — that would undo the
 * jump the press just made.
 */
export function releaseCue(previewing: boolean, cuePoint: number): CueAction | null {
  if (!previewing) return null;
  return { seekTo: cuePoint, playing: false, cuePoint };
}

/**
 * How far the drawn layer must slide, in pixels, to keep the head centred.
 *
 * The layer covers `OVERDRAW` spans centred on `anchor`, laid out so that with
 * no transform its middle is the middle of the strip. Sliding it by this puts
 * `progress` there instead.
 */
export function scrollOffset(
  progress: number,
  anchor: number,
  span: number,
  width: number,
): number {
  if (span <= 0 || width <= 0) return 0;
  return -((progress - anchor) / span) * width;
}

/**
 * Whether the playhead has run far enough that the layer must be redrawn.
 *
 * The layer reaches half its width either side of the anchor, and the strip
 * shows half a span either side of the head, so the hard limit is half a span.
 * Redrawing at half of that leaves a margin: a redraw that lands exactly as
 * the edge arrives is a redraw that sometimes arrives late.
 */
export function needsRedraw(progress: number, anchor: number, span: number): boolean {
  if (span <= 0) return false;
  return Math.abs(progress - anchor) > (span * (OVERDRAW - 1)) / 4;
}
