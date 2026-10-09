/**
 * The MEMORY cluster: store, call previous, call next, delete — and this
 * fork's own store every 16 bars.
 *
 * rekordbox's own arrangement, from `german.lang` and the Export key map:
 * `Set Memory Cue` (`M`) stores the cue point the transport's CUE or IN set,
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
import { MEMORY_TOLERANCE_MS, memoryCueAt, memoryCueNumber, nextMemoryCue, previousMemoryCue } from "@/lib/cues";
import { BEATS_PER_BAR, beatsBackMs, nearestBeatMs, type BeatGrid } from "@/lib/player";
import { useCueWriter } from "./useCueWriter";

export { READ_ONLY_REASON } from "./useCueWriter";

/** Memory cues and loops a track can hold: rekordbox's MEMORY list is ten. */
export const MEMORY_CUE_LIMIT = 10;

/** How far apart `storeEvery16Bars` puts its cues. */
const PHRASE_BEATS = 16 * BEATS_PER_BAR;

const isLoop = (cue: Cue) => cue.outMs > cue.positionMs;

export interface MemoryCueDeck {
  /** The loaded track's id, or `null` when the deck is empty. */
  trackId: string | null;
  cues: readonly Cue[];
  /** The playhead, in seconds, read at the moment a button goes down. */
  positionSeconds: () => number;
  seek: (seconds: number) => void;
  /** Sets the deck's loop, for a memory loop called: it plays as a loop. */
  setLoop?: ((inSeconds: number, outSeconds: number) => void) | undefined;
  /** Where CUE returns to, in seconds; what MEMORY stores. */
  cuePoint: number;
  setCuePoint: (seconds: number) => void;
  /** Rekordbox holds the database, so nothing here can write. */
  readOnly: boolean;
  onError?: ((message: string | null) => void) | undefined;
  /** The loaded track's beat grid, which `storeEvery16Bars` counts bars on. */
  grid?: BeatGrid | undefined;
  /** The grid the playhead snaps to with Q on, or `null` with it off. */
  quantiseTo?: BeatGrid | null | undefined;
}

export interface MemoryCueActions {
  /** Whether the controls do anything: a track is loaded and can be written. */
  canEdit: boolean;
  /** `store`, plus a beat grid to count the 16 bars on. */
  canStoreEvery16Bars: boolean;
  /** `Set Memory Cue`: stores the cue point as a memory cue. */
  store: () => void;
  /**
   * Replaces the memory cues with one at the playhead and one every 16 bars
   * before it, back to the start of the track.
   */
  storeEvery16Bars: () => void;
  /** `Call Previous Memory Cue`. */
  callPrevious: () => void;
  /** `Call Next Memory Cue`. */
  callNext: () => void;
  /** `Memory Cue 1` to `10`: the nth from the start of the track, one-based. */
  callNumber: (n: number) => void;
  /** `Delete Memory Cue`: the one under the playhead, if any. */
  deleteAtHead: () => void;
  /** The ✕ on a list row. */
  remove: (cue: Cue) => void;
}

export function useMemoryCues(deck: MemoryCueDeck): MemoryCueActions {
  const {
    trackId, cues, positionSeconds, seek, setLoop, cuePoint, setCuePoint, readOnly, onError, grid, quantiseTo,
  } = deck;
  const canEdit = trackId !== null && !readOnly;
  const canStoreEvery16Bars = canEdit && grid !== undefined && grid.times.length >= 2;
  const write = useCueWriter(onError);

  /**
   * MEMORY stores the cue point, never the playhead, playing or paused.
   * [OBS] rekordbox 7.2.19 `UiPlayer::eventMemoryCue` @0x101b9d91c reads
   * `DjEngineIF::getCueTime(channel, 0)` (the deck's current cue, the one CUE
   * and IN set) and writes that with `setCueTime`; it does not read the play
   * position or pause the deck. A DJ who wants the playhead stored presses
   * IN there first, which makes it the cue point (see `markLoopIn`).
   */
  const store = useCallback(() => {
    if (!canEdit || trackId === null) return;
    const positionMs = Math.round(Math.max(cuePoint, 0) * 1000);
    // [OBS] The same handler does nothing when a memory cue already sits on
    // the cue point, or when the track already has ten (memory cues and
    // memory loops together, @0x101b9dde8). rekordbox compares the exact
    // point; within the list's tolerance is the nearest the rows here can
    // tell apart.
    if (memoryCueAt(cues, positionMs) || cues.filter((c) => c.memory).length >= MEMORY_CUE_LIMIT) return;
    write((edits) => edits.addCue(trackId, "memory", positionMs));
  }, [canEdit, trackId, cuePoint, cues, write]);

  /**
   * This fork's own button, not rekordbox's: the playhead rather than the cue
   * point, so placing the head and pressing is the whole gesture. Memory
   * loops stay and count towards the ten, which go to the places nearest the
   * playhead. A cue already on one of the places is kept rather than deleted
   * and written again, so a second press writes nothing.
   */
  const storeEvery16Bars = useCallback(() => {
    if (!canStoreEvery16Bars || trackId === null || !grid) return;
    const head = Math.max(positionSeconds(), 0) * 1000;
    const from = quantiseTo ? nearestBeatMs(quantiseTo, head) : head;
    const loops = cues.filter((c) => c.memory && isLoop(c));
    const places = beatsBackMs(grid, from, PHRASE_BEATS)
      .filter((ms) => !memoryCueAt(loops, ms))
      .slice(0, Math.max(MEMORY_CUE_LIMIT - loops.length, 0));
    const stale = cues.filter((c) => c.memory && !isLoop(c) && c.id !== ""
      && !places.some((ms) => Math.abs(c.positionMs - ms) <= MEMORY_TOLERANCE_MS));
    const fresh = places.filter((ms) => !memoryCueAt(cues, ms));
    if (stale.length === 0 && fresh.length === 0) return;
    write(async (edits) => {
      // Deletes first, so the track never holds more than ten on the way.
      for (const cue of stale) await edits.deleteCue(cue.id);
      for (const ms of fresh) await edits.addCue(trackId, "memory", ms);
    });
  }, [canStoreEvery16Bars, trackId, grid, positionSeconds, quantiseTo, cues, write]);

  /**
   * Calling a memory cue moves the playhead there and makes it the cue
   * point, as a CDJ's CUE/LOOP CALL does [REF]: the point is called, not
   * merely visited, so CUE returns to it afterwards. A memory loop is
   * called as the loop: the deck goes round it from its in point.
   */
  const call = useCallback(
    (cue: Cue | null) => {
      if (!cue) return;
      const seconds = cue.positionMs / 1000;
      if (cue.outMs > cue.positionMs && setLoop) {
        setLoop(seconds, cue.outMs / 1000);
      } else {
        seek(seconds);
      }
      setCuePoint(seconds);
    },
    [seek, setLoop, setCuePoint],
  );

  const callPrevious = useCallback(() => {
    if (trackId === null) return;
    call(previousMemoryCue(cues, positionSeconds() * 1000));
  }, [trackId, cues, positionSeconds, call]);

  const callNext = useCallback(() => {
    if (trackId === null) return;
    call(nextMemoryCue(cues, positionSeconds() * 1000));
  }, [trackId, cues, positionSeconds, call]);

  const callNumber = useCallback(
    (n: number) => {
      if (trackId === null) return;
      call(memoryCueNumber(cues, n));
    },
    [trackId, cues, call],
  );

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

  return {
    canEdit, canStoreEvery16Bars, store, storeEvery16Bars, callPrevious, callNext, callNumber, deleteAtHead, remove,
  };
}
