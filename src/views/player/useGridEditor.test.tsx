/**
 * @vitest-environment jsdom
 *
 * The GRID EDIT cluster's own rules, above the arithmetic its buttons ask for.
 *
 * `src/lib/gridEdit.test.ts` covers what an edit does to a grid and
 * `src-tauri/src/grid.rs` covers what the backend writes; neither says what
 * the panel sends, or when it sends nothing. That is what is here: which
 * button carries the CUT point and which carries the playhead, that a held
 * key is one edit rather than thirty, that a run of taps commits once, and
 * what a locked or read-only grid still allows.
 *
 * The backend records what it was asked for and answers with whatever state
 * a test wants, so nothing here needs an analysis file.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { __setBackend } from "@/ipc/client";
import type { Backend, GridState } from "@/ipc/types";
import { SHIFT_MS, STRETCH_X100, TAP_GAP_MS } from "@/lib/gridEdit";
import { useGridEditor, type GridEditorActions } from "./useGridEditor";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

/** A grid the panel is happy with: 128.00 BPM over four hundred beats. */
const gridState = (patch: Partial<GridState> = {}): GridState => ({
  bpmX100: 12_800,
  beats: 400,
  canUndo: false,
  canRedo: false,
  locked: false,
  ...patch,
});

let host: HTMLDivElement;
let root: Root;
/** What the hook returned on the last render. */
let grid: GridEditorActions;
/** The playhead the panel reads when a button goes down, in milliseconds. */
let playhead: number;
/** `performance.now()`, moved by hand so a tap run is exact. */
let clock: number;
let setState: ReturnType<typeof vi.fn>;
let onError: ReturnType<typeof vi.fn>;
let edits: {
  gridEdit: ReturnType<typeof vi.fn>;
  gridUndo: ReturnType<typeof vi.fn>;
  gridRedo: ReturnType<typeof vi.fn>;
  gridLock: ReturnType<typeof vi.fn>;
};

interface ProbeProps {
  trackId: string | null;
  state: GridState | null;
  readOnly: boolean;
}

/** Mounts the hook and hands the test what it returned. */
function Probe({ trackId, state, readOnly }: ProbeProps) {
  grid = useGridEditor({
    trackId,
    deck: "a",
    state,
    setState,
    positionMs: () => playhead,
    readOnly,
    onError,
  });
  return null;
}

function mount(props: Partial<ProbeProps> = {}) {
  const full: ProbeProps = { trackId: "t1", state: gridState(), readOnly: false, ...props };
  act(() => {
    root.render(<Probe {...full} />);
  });
}

/**
 * Lets the writes a button fires off reach the backend and come back. Every
 * step of `run` is a microtask — the backend is already resolved here — so a
 * few turns of the queue is the whole round trip.
 */
const settle = () =>
  act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });

/** Moves the clock and the timers together, the way time actually passes. */
function tick(ms: number) {
  clock += ms;
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  clock = 1_000;
  vi.spyOn(performance, "now").mockImplementation(() => clock);
  playhead = 0;
  setState = vi.fn();
  onError = vi.fn();
  edits = {
    gridEdit: vi.fn(() => Promise.resolve(gridState())),
    gridUndo: vi.fn(() => Promise.resolve(gridState())),
    gridRedo: vi.fn(() => Promise.resolve(gridState())),
    gridLock: vi.fn(() => Promise.resolve(gridState())),
  };
  __setBackend({ edits } as unknown as Backend);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  __setBackend(null);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("one edit at a time", () => {
  it("drops the presses that land while a write is still in the air", async () => {
    // A held key repeats thirty times a second; each repeat used to be
    // another edit, and another undo step, before the first came back.
    let land = (_state: GridState) => {};
    edits.gridEdit.mockImplementation(
      () =>
        new Promise<GridState>((resolve) => {
          land = resolve;
        }),
    );
    mount();

    act(() => grid.double());
    act(() => grid.double());
    act(() => grid.halve());
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledTimes(1);
    expect(edits.gridEdit).toHaveBeenCalledWith("t1", { kind: "double" }, { deck: "a" });

    act(() => land(gridState({ bpmX100: 25_600 })));
    await settle();
    expect(setState).toHaveBeenCalledWith(gridState({ bpmX100: 25_600 }));

    // Once it has landed the next press goes through as normal.
    act(() => grid.halve());
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledTimes(2);
    expect(edits.gridEdit).toHaveBeenLastCalledWith("t1", { kind: "halve" }, { deck: "a" });
  });
});

describe("the CUT point", () => {
  it("is the playhead where it was pressed, and goes away on a second press", () => {
    mount();
    playhead = 9_000.6;
    act(() => grid.toggleCut());
    expect(grid.cutMs).toBe(9_001);
    // Moving on does not drag the point along with it.
    playhead = 20_000;
    expect(grid.cutMs).toBe(9_001);
    act(() => grid.toggleCut());
    expect(grid.cutMs).toBeNull();
  });

  it("rides along on every whole-grid edit once it is set", async () => {
    mount();
    playhead = 9_000;
    act(() => grid.toggleCut());
    playhead = 30_000;

    const sent = async (press: () => void) => {
      act(press);
      await settle();
      return edits.gridEdit.mock.calls[edits.gridEdit.mock.calls.length - 1];
    };

    expect(await sent(() => grid.mark())).toEqual([
      "t1", { kind: "downbeat", timeMs: 30_000 }, { deck: "a", fromMs: 9_000 },
    ]);
    expect(await sent(() => grid.shift(1))).toEqual([
      "t1", { kind: "nudge", ms: SHIFT_MS }, { deck: "a", fromMs: 9_000 },
    ]);
    expect(await sent(() => grid.shift(-1))).toEqual([
      "t1", { kind: "nudge", ms: -SHIFT_MS }, { deck: "a", fromMs: 9_000 },
    ]);
    expect(await sent(() => grid.stretch(1))).toEqual([
      "t1", { kind: "stretch", byX100: STRETCH_X100 }, { deck: "a", fromMs: 9_000 },
    ]);
    expect(await sent(() => grid.double())).toEqual([
      "t1", { kind: "double" }, { deck: "a", fromMs: 9_000 },
    ]);
    expect(await sent(() => grid.halve())).toEqual([
      "t1", { kind: "halve" }, { deck: "a", fromMs: 9_000 },
    ]);
  });

  it("is left out of the options entirely while nothing is cut", async () => {
    mount();
    playhead = 4_000;
    act(() => grid.double());
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledWith("t1", { kind: "double" }, { deck: "a" });
  });
});

describe("the two snap buttons", () => {
  it("|↓| moves the whole grid; ||↓ moves it from the playhead on", async () => {
    // The only thing telling the two buttons apart, and what each is for: one
    // obeys the CUT point, the other is its own cut at the playhead.
    mount();
    playhead = 10_500.4;

    act(() => grid.alignAll());
    await settle();
    expect(edits.gridEdit).toHaveBeenLastCalledWith(
      "t1", { kind: "align", timeMs: 10_500 }, { deck: "a" },
    );

    act(() => grid.alignHere());
    await settle();
    expect(edits.gridEdit).toHaveBeenLastCalledWith(
      "t1", { kind: "align", timeMs: 10_500 }, { deck: "a", fromMs: 10_500 },
    );
  });

  it("||↓ cuts at the playhead whatever the CUT point says", async () => {
    mount();
    playhead = 9_000;
    act(() => grid.toggleCut());
    playhead = 40_000;

    act(() => grid.alignAll());
    await settle();
    expect(edits.gridEdit).toHaveBeenLastCalledWith(
      "t1", { kind: "align", timeMs: 40_000 }, { deck: "a", fromMs: 9_000 },
    );

    act(() => grid.alignHere());
    await settle();
    expect(edits.gridEdit).toHaveBeenLastCalledWith(
      "t1", { kind: "align", timeMs: 40_000 }, { deck: "a", fromMs: 40_000 },
    );
  });
});

describe("TAP", () => {
  it("is one tempo edit for the whole run, anchored where the first tap landed", async () => {
    mount();
    playhead = 5_000;

    act(() => grid.tap());
    // One tap describes no tempo at all.
    expect(grid.tapBpmX100).toBeNull();

    // The track runs on under the tapping hand; the anchor does not follow it.
    playhead = 5_500;
    tick(500);
    act(() => grid.tap());
    expect(grid.tapBpmX100).toBe(12_000);

    playhead = 6_000;
    tick(500);
    act(() => grid.tap());
    expect(grid.tapBpmX100).toBe(12_000);
    // Nothing is written while the tapping is still going on.
    expect(edits.gridEdit).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(TAP_GAP_MS);
    });
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledTimes(1);
    expect(edits.gridEdit).toHaveBeenCalledWith(
      "t1", { kind: "tempo", bpmX100: 12_000, anchorMs: 5_000 }, { deck: "a" },
    );
    // And the field goes back to showing the grid rather than the taps.
    expect(grid.tapBpmX100).toBeNull();
  });

  it("commits nothing for a single tap", async () => {
    mount();
    playhead = 1_000;
    act(() => grid.tap());
    act(() => {
      vi.advanceTimersByTime(TAP_GAP_MS);
    });
    await settle();
    expect(edits.gridEdit).not.toHaveBeenCalled();
  });

  it("carries the CUT point like any other edit", async () => {
    mount();
    playhead = 80_000;
    act(() => grid.toggleCut());
    act(() => grid.tap());
    tick(400);
    act(() => grid.tap());
    act(() => {
      vi.advanceTimersByTime(TAP_GAP_MS);
    });
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledWith(
      "t1", { kind: "tempo", bpmX100: 15_000, anchorMs: 80_000 }, { deck: "a", fromMs: 80_000 },
    );
  });
});

describe("loading another track", () => {
  it("clears the CUT point and the taps in progress", async () => {
    mount();
    playhead = 9_000;
    act(() => grid.toggleCut());
    act(() => grid.tap());
    tick(400);
    act(() => grid.tap());
    expect(grid.cutMs).toBe(9_000);
    expect(grid.tapBpmX100).toBe(15_000);

    mount({ trackId: "t2" });
    expect(grid.cutMs).toBeNull();
    expect(grid.tapBpmX100).toBeNull();

    // And the run that was in the air commits nothing against the new track,
    // which never heard the tapping.
    act(() => {
      vi.advanceTimersByTime(TAP_GAP_MS);
    });
    await settle();
    expect(edits.gridEdit).not.toHaveBeenCalled();
  });
});

describe("what blocks an edit", () => {
  it("a locked grid takes no edit, but the lock itself still comes off", async () => {
    mount({ state: gridState({ locked: true }) });
    expect(grid.hasGrid).toBe(true);
    expect(grid.canEdit).toBe(false);

    act(() => grid.double());
    act(() => grid.mark());
    act(() => grid.tap());
    await settle();
    expect(edits.gridEdit).not.toHaveBeenCalled();

    // The lock is the one control a locked grid has to keep: it is how it
    // gets unlocked.
    act(() => grid.toggleLock());
    await settle();
    expect(edits.gridLock).toHaveBeenCalledWith("t1", false);
  });

  it("a locked grid still undoes and redoes: the lock is on editing, not on history", async () => {
    mount({ state: gridState({ locked: true, canUndo: true, canRedo: true }) });
    act(() => grid.undo());
    await settle();
    expect(edits.gridUndo).toHaveBeenCalledWith("t1", "a");
    act(() => grid.redo());
    await settle();
    expect(edits.gridRedo).toHaveBeenCalledWith("t1", "a");
  });

  it("a read-only library takes nothing at all, history included", async () => {
    mount({ readOnly: true });
    expect(grid.hasGrid).toBe(true);
    expect(grid.canEdit).toBe(false);
    act(() => grid.double());
    act(() => grid.alignHere());
    act(() => grid.undo());
    act(() => grid.redo());
    await settle();
    expect(edits.gridEdit).not.toHaveBeenCalled();
    expect(edits.gridUndo).not.toHaveBeenCalled();
    expect(edits.gridRedo).not.toHaveBeenCalled();
  });

  it("a track with no grid has nothing to edit, cut or lock", async () => {
    mount({ state: null });
    expect(grid.hasGrid).toBe(false);
    expect(grid.canEdit).toBe(false);
    act(() => grid.double());
    act(() => grid.toggleCut());
    act(() => grid.undo());
    act(() => grid.toggleLock());
    await settle();
    expect(grid.cutMs).toBeNull();
    expect(edits.gridEdit).not.toHaveBeenCalled();
    expect(edits.gridUndo).not.toHaveBeenCalled();
    expect(edits.gridLock).not.toHaveBeenCalled();
  });

  it("a grid of no beats is no grid, and an empty deck is neither", () => {
    mount({ state: gridState({ beats: 0 }) });
    expect(grid.hasGrid).toBe(false);
    mount({ trackId: null });
    expect(grid.hasGrid).toBe(false);
  });
});

describe("a write that fails", () => {
  it("says what went wrong, falls back when the failure says nothing, and clears on the next one", async () => {
    mount();

    edits.gridEdit.mockRejectedValueOnce(new Error("the grid is locked"));
    act(() => grid.double());
    await settle();
    expect(onError).toHaveBeenLastCalledWith("the grid is locked");
    expect(setState).not.toHaveBeenCalled();

    // A rejection carrying no message of its own still has to read as
    // something; the panel has no other place to say so.
    edits.gridEdit.mockRejectedValueOnce({});
    act(() => grid.halve());
    await settle();
    expect(onError).toHaveBeenLastCalledWith("The beat grid could not be saved.");

    act(() => grid.halve());
    await settle();
    expect(onError).toHaveBeenLastCalledWith(null);
    expect(setState).toHaveBeenCalledWith(gridState());
  });

  it("lets the next press through rather than jamming the guard", async () => {
    mount();
    edits.gridEdit.mockRejectedValueOnce(new Error("no"));
    act(() => grid.double());
    await settle();
    act(() => grid.double());
    await settle();
    expect(edits.gridEdit).toHaveBeenCalledTimes(2);
  });
});
