/**
 * Memory cue navigation, as pure functions over a track's cue list.
 *
 * The deck's MEMORY cluster is three buttons and a key each — `B` calls the
 * memory cue before the playhead, `N` the one after, `X` deletes the one it
 * is standing on, `M` stores the cue point as one — all from rekordbox's own
 * Export key map. What "before", "after" and "standing on" mean is decided
 * here, where it can be tested without a deck.
 */
import type { Cue } from "@/ipc/types";

/**
 * How close to a memory cue the playhead has to be to count as on it, in
 * milliseconds.
 *
 * Wide enough that a playhead that has just been called to a cue is still on
 * it after the engine's rounding, and narrow enough that two cues a beat
 * apart are never confused. The same 20 ms the CUE button uses to decide
 * whether it is on the cue point. [ASSUME] rekordbox's own tolerance is not
 * documented; nothing here depends on matching it exactly.
 */
export const MEMORY_TOLERANCE_MS = 20;

/** The memory cues of a list, in position order. Hot cues are not called. */
function memoryCues(cues: readonly Cue[]): Cue[] {
  return cues.filter((cue) => cue.memory).sort((a, b) => a.positionMs - b.positionMs);
}

/**
 * The first memory cue after the playhead, or `null` when there is none.
 *
 * Strictly after, by more than the tolerance: called to a cue and pressed
 * again, ▶ moves to the next one rather than staying put.
 */
export function nextMemoryCue(cues: readonly Cue[], positionMs: number): Cue | null {
  for (const cue of memoryCues(cues)) {
    if (cue.positionMs > positionMs + MEMORY_TOLERANCE_MS) return cue;
  }
  return null;
}

/** The last memory cue before the playhead, or `null`. The mirror of `next`. */
export function previousMemoryCue(cues: readonly Cue[], positionMs: number): Cue | null {
  let found: Cue | null = null;
  for (const cue of memoryCues(cues)) {
    if (cue.positionMs < positionMs - MEMORY_TOLERANCE_MS) found = cue;
    else break;
  }
  return found;
}

/**
 * The memory cue the playhead is standing on, or `null`.
 *
 * What ✕ deletes. Only a cue within the tolerance counts: a delete that took
 * the nearest cue however far away would be a surprise with a track's worth
 * of cues at stake, and rekordbox's own X does nothing away from a cue.
 */
export function memoryCueAt(cues: readonly Cue[], positionMs: number): Cue | null {
  let best: Cue | null = null;
  for (const cue of cues) {
    if (!cue.memory) continue;
    const distance = Math.abs(cue.positionMs - positionMs);
    if (distance > MEMORY_TOLERANCE_MS) continue;
    if (!best || distance < Math.abs(best.positionMs - positionMs)) best = cue;
  }
  return best;
}
