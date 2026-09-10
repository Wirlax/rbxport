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
 * Every write goes through `useCueWriter`, which the hot cue pads share:
 * nothing is shown ahead of the backend, which re-reads the track and says
 * so, `useTrackCues` refetches, and the markers and the list follow from the
 * same array.
 */
import { useCallback } from "react";

import type { Cue } from "@/ipc/types";
import { memoryCueAt, nextMemoryCue, previousMemoryCue } from "@/lib/cues";
import { useCueWriter } from "./useCueWriter";

export { READ_ONLY_REASON } from "./useCueWriter";

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

export function useMemoryCues(deck: MemoryCueDeck): MemoryCueActions {
  const { trackId, cues, positionSeconds, seek, cuePoint, setCuePoint, readOnly, onError } = deck;
  const canEdit = trackId !== null && !readOnly;
  const write = useCueWriter(onError);

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
