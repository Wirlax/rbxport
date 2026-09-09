/**
 * @vitest-environment jsdom
 *
 * What a view does when it is opened before the library is up.
 *
 * The backend reads the library on its own thread, so the first `open_view`
 * can arrive before there is anything to answer with and comes back
 * "The library has not finished loading yet." That is not a failure worth
 * showing anyone — it is a race the app is expected to lose sometimes and
 * recover from. It did not recover: the open effect keys on the spec, so a
 * view that failed stayed at zero rows until the spec changed, and a table
 * with a count of zero draws nothing at any scroll position.
 *
 * `App.tsx` already learned this for the tree; these hold the same line for
 * the rows.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Backend, RowDto, ViewSpec } from "@/ipc/types";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let host: HTMLDivElement;
let root: Root;
let useTrackView: typeof import("./useTrackView").useTrackView;
let setBackend: typeof import("@/ipc/client").__setBackend;

/** Everything the test drives from outside. */
let ready: boolean;
let readyListeners: Set<() => void>;
let opens: number;

const SPEC: ViewSpec = {
  source: { kind: "collection" },
  sort: "trackNo",
  descending: false,
  query: "",
};

function row(i: number): RowDto {
  return {
    id: String(i),
    trackNo: i + 1,
    title: `Track ${i}`,
    artist: "",
    album: "",
    genre: "",
    label: "",
    comment: "",
    bpmX100: 0,
    key: "",
    durationSec: 0,
    rating: 0,
    analysed: 0,
    dateAdded: "",
    releaseDate: "",
    cues: "",
    hasArtwork: false,
    artworkHue: 0,
  };
}

/** A backend that refuses everything until the library is released. */
function makeBackend(): Backend {
  return {
    openView: (_spec: ViewSpec) => {
      opens += 1;
      return ready
        ? Promise.resolve({ viewId: 1, len: 500, gen: 1 })
        : Promise.reject(new Error("The library has not finished loading yet."));
    },
    fetchRows: (_viewId: number, offset: number, len: number) =>
      ready
        ? Promise.resolve(Array.from({ length: len }, (_, i) => row(offset + i)))
        : Promise.reject(new Error("The library has not finished loading yet.")),
    onLibraryReady: (listener: () => void) => {
      readyListeners.add(listener);
      return () => readyListeners.delete(listener);
    },
  } as unknown as Backend;
}

function release() {
  ready = true;
  for (const listener of readyListeners) listener();
}

/** Drains the promise queue; several passes, the chain is a few deep. */
const settle = async () => {
  await act(async () => {
    for (let i = 0; i < 30; i++) await Promise.resolve();
  });
};

let latest: { count: number; error: string | null; rowAt: (i: number) => RowDto | undefined };

function Probe() {
  const view = useTrackView(SPEC);
  latest = view;
  // A table asks for the window it is showing; this stands in for that.
  view.ensureRange(0, 32);
  return null;
}

beforeEach(async () => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  vi.resetModules();
  ready = false;
  readyListeners = new Set();
  opens = 0;
  ({ __setBackend: setBackend } = await import("@/ipc/client"));
  setBackend(makeBackend());
  ({ useTrackView } = await import("./useTrackView"));
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  setBackend(null);
});

describe("useTrackView, against a library that is not up yet", () => {
  it("opens the view itself once the library becomes ready", async () => {
    act(() => {
      root.render(<Probe />);
    });
    await settle();
    // Correct so far: there genuinely is nothing to show.
    expect(latest.count).toBe(0);

    release();
    await settle();

    // The spec never changed. Nobody clicked anything. The rows must arrive
    // anyway, or the window stays blank until the app is restarted.
    expect(latest.count).toBe(500);
    expect(latest.error).toBeNull();
  });

  it("fills the rows of the window it was already showing", async () => {
    act(() => {
      root.render(<Probe />);
    });
    await settle();
    release();
    await settle();
    await settle();

    expect(latest.rowAt(0)?.title).toBe("Track 0");
  });

  it("does not reopen a view that opened perfectly well", async () => {
    ready = true;
    act(() => {
      root.render(<Probe />);
    });
    await settle();
    expect(latest.count).toBe(500);

    const before = opens;
    // A later ready event — a library reload, say — must not make every open
    // view refetch itself.
    for (const listener of readyListeners) listener();
    await settle();
    expect(opens).toBe(before);
  });
});
