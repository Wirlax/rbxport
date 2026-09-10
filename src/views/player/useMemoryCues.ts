/**
 * The MEMORY cluster: store, call previous, call next, delete.
 *
 * rekordbox's own arrangement, from `german.lang` and the Export key map:
 * `Set Memory Cue` (`M`) stores the cue point the transport's CUE has set,
 * `Call Previous Memory Cue` (`B`) and `Call Next Memory Cue` (`N`) move the
 * playhead to the memory cue either side of it, and `Delete Memory Cue` (`X`)
 * removes the one the playhead is standing on. The list beside the deck has
 * a ✕ per row for the rest.
 *
 * Every write goes through the backend's cue commands and nothing is shown
 * ahead of it: the backend re-reads the track and says so, `useTrackCues`
 * refetches, and the markers and the list follow from the same array. A cue
 * write is one indexed row, so there is no reload to wait through.
 */
import { useCallback, useRef } from "react";

import type { Cue } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { memoryCueAt, nextMemoryCue, previousMemoryCue } from "@/lib/cues";

/** What the interface says when the library cannot be written to. */
export const READ_ONLY_REASON = "rekordbox is running, so the library is open read-only.";

export interface MemoryCueDeck {
  /** The loaded track's id, or `null` when the deck is empty. */
  trackId: string | null;
  cues: readonly Cue[];
  /** The playhead, in seconds, read at the moment a button goes down. */
  positionSeconds: () => number;
  seek: (seconds: number) => void;
  /** Where CUE returns to, in seconds; what MEMORY stores. */
  cuePoint: number;
  setCuePoint: (seconds: number) => void;
  /** Rekordbox holds the database, so nothing here can write. */
  readOnly: boolean;
  onError?: ((message: string | null) => void) | undefined;
}

export interface MemoryCueActions {
  /** Whether the controls do anything: a track is loaded and can be written. */
  canEdit: boolean;
  /** `Set Memory Cue`: stores the cue point as a memory cue. */
  store: () => void;
  /** `Call Previous Memory Cue`. */
  callPrevious: () => void;
  /** `Call Next Memory Cue`. */
  callNext: () => void;
  /** `Delete Memory Cue`: the one under the playhead, if any. */
  deleteAtHead: () => void;
  /** The ✕ on a list row. */
  remove: (cue: Cue) => void;
}

function describe(error: unknown): string {
  if (error instanceof Error && error.message.trim() !== "") return error.message;
  if (typeof error === "object" && error !== null) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim() !== "") return message;
  }
  return "That cue could not be saved.";
}

export function useMemoryCues(deck: MemoryCueDeck): MemoryCueActions {
  const { trackId, cues, positionSeconds, seek, cuePoint, setCuePoint, readOnly, onError } = deck;
  const canEdit = trackId !== null && !readOnly;
  /**
   * One write at a time. A held key repeats thirty times a second, and each
   * repeat used to be another cue at the same point until the first came
   * back; the guard drops everything after the first press.
   */
  const inFlight = useRef(false);

  const write = useCallback(
    (action: (edits: Awaited<ReturnType<typeof getBackend>>["edits"]) => Promise<unknown>) => {
      if (inFlight.current) return;
      inFlight.current = true;
      void (async () => {
        try {
          const backend = await getBackend();
          await action(backend.edits);
          onError?.(null);
        } catch (error) {
          onError?.(describe(error));
        } finally {
          inFlight.current = false;
        }
      })();
    },
    [onError],
  );

  const store = useCallback(() => {
    if (!canEdit || trackId === null) return;
    const positionMs = Math.round(Math.max(cuePoint, 0) * 1000);
    // One memory cue per point. [ASSUME] Whether rekordbox stacks a second
    // cue on the same millisecond is not recorded; a second row at the same
    // point is nothing the list or the waveform could show, so it is not
    // written.
    if (memoryCueAt(cues, positionMs)) return;
    write((edits) => edits.addCue(trackId, "memory", positionMs));
  }, [canEdit, trackId, cuePoint, cues, write]);

  /**
   * Calling a memory cue moves the playhead there and makes it the cue
   * point, as a CDJ's CUE/LOOP CALL does [REF]: the point is called, not
   * merely visited, so CUE returns to it afterwards.
   */
  const call = useCallback(
    (cue: Cue | null) => {
      if (!cue) return;
      const seconds = cue.positionMs / 1000;
      seek(seconds);
      setCuePoint(seconds);
    },
    [seek, setCuePoint],
  );

  const callPrevious = useCallback(() => {
    if (trackId === null) return;
    call(previousMemoryCue(cues, positionSeconds() * 1000));
  }, [trackId, cues, positionSeconds, call]);

  const callNext = useCallback(() => {
    if (trackId === null) return;
    call(nextMemoryCue(cues, positionSeconds() * 1000));
  }, [trackId, cues, positionSeconds, call]);

  const remove = useCallback(
    (cue: Cue) => {
      // An empty id is a cue the backend cannot address; the row shows it
      // without a ✕, and a key press finds nothing to do.
      if (!canEdit || cue.id === "") return;
      write((edits) => edits.deleteCue(cue.id));
    },
    [canEdit, write],
  );

  const deleteAtHead = useCallback(() => {
    const cue = memoryCueAt(cues, positionSeconds() * 1000);
    if (cue) remove(cue);
  }, [cues, positionSeconds, remove]);

  return { canEdit, store, callPrevious, callNext, deleteAtHead, remove };
}
