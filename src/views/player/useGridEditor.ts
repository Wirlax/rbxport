/**
 * The GRID EDIT cluster: what each button does to the loaded track's grid.
 *
 * Every button is one of `rbl_anlz::grid`'s edits sent through the backend,
 * which rewrites the analysis file and says so; `useTrackGrid` refetches and
 * the beats on the waveform, the BPM field and the undo/redo buttons follow
 * from that. Nothing is shown ahead of the write.
 *
 * What the buttons are, in rekordbox's own terms where the Export key map
 * names them (`Shift Beatgrid left/right`, `Shift Beatgrid to the center`)
 * and by what the manual says the rest do:
 *
 * - `mark` (the red bar): the beat nearest the playhead becomes beat 1.
 * - `TAP`: tap along; the tempo is the mean of the last taps and the grid
 *   is laid out again with a beat where the first tap landed. Committed
 *   once the tapping stops, so a run of taps is one edit and one undo.
 * - `shift`: the grid a millisecond earlier or later.
 * - `stretch`: the tempo 0.01 BPM slower (widen) or faster (narrow), the
 *   first beat held.
 * - `double` / `halve`: the tempo, keeping the downbeat.
 * - `alignAll` (`|↓|`): the grid moved so the beat nearest the playhead
 *   lands on it — `Shift Beatgrid to the center`, the playhead being the
 *   centre of the detail waveform.
 * - `alignHere` (`||↓`): the same, but only from the playhead on; the beats
 *   before it stay [ASSUME: rekordbox's two snap buttons were read off the
 *   capture as a whole-grid one and a from-here one].
 * - `cut` (the scissors): sets the point from which the other edits apply,
 *   so a tempo change part-way through a DJ edit can be gridded without
 *   moving the beats before it [ASSUME: rekordbox 6's "adjust from this
 *   position on" is what the scissors stand for]. Pressing it again clears
 *   the point. It is the deck's, not the file's: a grid records tempo per
 *   beat, not where it was cut.
 * - `lock`: no edit is written while it is on; see `src-tauri/src/grid.rs`
 *   for where the lock lives.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import type { DeckId, GridEdit, GridEditOptions, GridState } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { SHIFT_MS, STRETCH_X100, TAP_GAP_MS, tapTempo, withTap } from "@/lib/gridEdit";

export interface GridEditorDeck {
  /** The loaded track's id, or `null` when the deck is empty. */
  trackId: string | null;
  deck: DeckId;
  /** From `useTrackGrid`: `null` while the track has no grid. */
  state: GridState | null;
  setState: (state: GridState) => void;
  /** The playhead, in milliseconds, read at the moment a button goes down. */
  positionMs: () => number;
  /** rekordbox holds the database, so nothing here can write. */
  readOnly: boolean;
  onError?: ((message: string | null) => void) | undefined;
}

export interface GridEditorActions {
  /** The panel's state as `useTrackGrid` holds it, for the buttons to read. */
  state: GridState | null;
  /** A track with a grid is loaded: the lock and undo/redo mean something. */
  hasGrid: boolean;
  /** And it can be written: not read-only, not locked. */
  canEdit: boolean;
  /** Where the from-here edits start, in milliseconds, or `null` for the whole grid. */
  cutMs: number | null;
  toggleCut: () => void;
  /** The tempo the taps so far describe, shown in the BPM field while tapping. */
  tapBpmX100: number | null;
  tap: () => void;
  mark: () => void;
  shift: (direction: -1 | 1) => void;
  stretch: (direction: -1 | 1) => void;
  double: () => void;
  halve: () => void;
  alignAll: () => void;
  alignHere: () => void;
  undo: () => void;
  redo: () => void;
  toggleLock: () => void;
}

function describe(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim() !== "") return error.message;
  if (typeof error === "object" && error !== null) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim() !== "") return message;
  }
  return fallback;
}

export function useGridEditor(deck: GridEditorDeck): GridEditorActions {
  const { trackId, deck: deckId, state, setState, positionMs, readOnly, onError } = deck;
  const hasGrid = trackId !== null && state !== null && state.beats > 0;
  const canEdit = hasGrid && !readOnly && !state.locked;
  const [cutMs, setCutMs] = useState<number | null>(null);
  const [taps, setTaps] = useState<number[]>([]);
  /** Where the first tap of the run landed in the track. */
  const tapAnchor = useRef(0);
  const tapTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // One write at a time: a held key repeats thirty times a second, and each
  // repeat used to be another edit before the first came back.
  const inFlight = useRef(false);

  // A cut and a run of taps belong to the track they were made on.
  useEffect(() => {
    setCutMs(null);
    setTaps([]);
    if (tapTimer.current) clearTimeout(tapTimer.current);
    tapTimer.current = null;
  }, [trackId]);

  const run = useCallback(
    (action: (edits: Awaited<ReturnType<typeof getBackend>>["edits"]) => Promise<GridState>) => {
      if (inFlight.current) return;
      inFlight.current = true;
      void (async () => {
        try {
          const backend = await getBackend();
          setState(await action(backend.edits));
          onError?.(null);
        } catch (error) {
          onError?.(describe(error, "The beat grid could not be saved."));
        } finally {
          inFlight.current = false;
        }
      })();
    },
    [setState, onError],
  );

  const edit = useCallback(
    (change: GridEdit, fromMs: number | null) => {
      if (!canEdit || trackId === null) return;
      const options: GridEditOptions = { deck: deckId };
      if (fromMs !== null) options.fromMs = fromMs;
      run((edits) => edits.gridEdit(trackId, change, options));
    },
    [canEdit, trackId, deckId, run],
  );

  const mark = useCallback(() => edit({ kind: "downbeat", timeMs: Math.round(positionMs()) }, cutMs), [edit, positionMs, cutMs]);
  const shift = useCallback((direction: -1 | 1) => edit({ kind: "nudge", ms: SHIFT_MS * direction }, cutMs), [edit, cutMs]);
  const stretch = useCallback(
    (direction: -1 | 1) => edit({ kind: "stretch", byX100: STRETCH_X100 * direction }, cutMs),
    [edit, cutMs],
  );
  const double = useCallback(() => edit({ kind: "double" }, cutMs), [edit, cutMs]);
  const halve = useCallback(() => edit({ kind: "halve" }, cutMs), [edit, cutMs]);
  const alignAll = useCallback(() => edit({ kind: "align", timeMs: Math.round(positionMs()) }, cutMs), [edit, positionMs, cutMs]);
  const alignHere = useCallback(() => {
    const at = Math.round(positionMs());
    edit({ kind: "align", timeMs: at }, at);
  }, [edit, positionMs]);

  const tap = useCallback(() => {
    if (!canEdit) return;
    const now = performance.now();
    const next = withTap(taps, now);
    if (next.length === 1) tapAnchor.current = Math.round(positionMs());
    setTaps(next);
    if (tapTimer.current) clearTimeout(tapTimer.current);
    // Committed when the tapping stops: the gap that ends a run of taps
    // is the one that starts a new one.
    tapTimer.current = setTimeout(() => {
      tapTimer.current = null;
      setTaps([]);
      const bpmX100 = tapTempo(next);
      if (bpmX100 !== null) edit({ kind: "tempo", bpmX100, anchorMs: tapAnchor.current }, cutMs);
    }, TAP_GAP_MS);
  }, [canEdit, taps, positionMs, edit, cutMs]);
  useEffect(
    () => () => {
      if (tapTimer.current) clearTimeout(tapTimer.current);
    },
    [],
  );

  const undo = useCallback(() => {
    if (!hasGrid || readOnly || trackId === null) return;
    run((edits) => edits.gridUndo(trackId, deckId));
  }, [hasGrid, readOnly, trackId, deckId, run]);
  const redo = useCallback(() => {
    if (!hasGrid || readOnly || trackId === null) return;
    run((edits) => edits.gridRedo(trackId, deckId));
  }, [hasGrid, readOnly, trackId, deckId, run]);
  const toggleLock = useCallback(() => {
    if (!hasGrid || trackId === null || state === null) return;
    run((edits) => edits.gridLock(trackId, !state.locked));
  }, [hasGrid, trackId, state, run]);

  const toggleCut = useCallback(() => {
    if (!hasGrid) return;
    setCutMs((current) => (current === null ? Math.round(positionMs()) : null));
  }, [hasGrid, positionMs]);

  return {
    state,
    hasGrid,
    canEdit,
    cutMs,
    toggleCut,
    tapBpmX100: taps.length >= 2 ? tapTempo(taps) : null,
    tap,
    mark,
    shift,
    stretch,
    double,
    halve,
    alignAll,
    alignHere,
    undo,
    redo,
    toggleLock,
  };
}
