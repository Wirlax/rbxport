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

/** Zoom levels, in bars across, that the +/- buttons step through. */
export const ZOOM_STEPS = [2, 4, 8, 12, 16, 32, 64] as const;

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

/** The slice of the track a window of `span` centred on `at` covers. */
export function windowAround(at: number, span: number): { from: number; to: number } {
  const half = Math.min(span, 1) / 2;
  const centre = Math.min(Math.max(at, half), 1 - half);
  return { from: centre - half, to: centre + half };
}

/**
 * Where the playhead sits inside that window, as a percentage.
 *
 * Centred, except near either end where the window stops moving and the head
 * crosses it instead — which is what a CDJ does and what makes the start and
 * end of a track reachable.
 */
export function headPercent(at: number, span: number): number {
  const width = Math.min(span, 1);
  const half = width / 2;
  if (at <= half) return (at / width) * 100;
  if (at >= 1 - half) return ((at - (1 - width)) / width) * 100;
  return 50;
}

/**
 * `-5:38.1` — time remaining, tenths in a smaller face.
 *
 * Returned split so the caller can size the fraction differently, which is how
 * rekordbox draws it.
 */
export function splitTime(seconds: number): { main: string; tenths: string } {
  const safe = Number.isFinite(seconds) ? Math.max(seconds, 0) : 0;
  const whole = Math.floor(safe);
  const tenths = Math.floor((safe - whole) * 10);
  const minutes = Math.floor(whole / 60);
  const rest = whole % 60;
  return { main: `${minutes}:${String(rest).padStart(2, "0")}`, tenths: String(tenths) };
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
