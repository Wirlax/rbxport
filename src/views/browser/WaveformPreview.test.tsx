/**
 * @vitest-environment jsdom
 *
 * What a row costs the backend while a scroll goes past it.
 *
 * Each of these mounts a preview and asks one question: did it call
 * `track_waveform`? A flick through a big playlist mounts thousands of rows,
 * and the answer used to be yes for every one of them — an IPC round trip and
 * an analysis file read on the same blocking pool `fetch_rows` uses, for a row
 * nobody saw. That is what made the list come up blank after a fast scroll.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Backend } from "@/ipc/types";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let host: HTMLDivElement;
let root: Root;
let asked: string[];
let WaveformPreview: typeof import("./WaveformPreview").WaveformPreview;
/**
 * Taken from the same module graph the component imports.
 *
 * `vi.resetModules()` gives the dynamic import below a fresh `ipc/client`, and
 * a `__setBackend` held from the outer import would be setting the backend on
 * a different copy of that module — which is how this first read as "the row
 * never asks".
 */
let setBackend: typeof import("@/ipc/client").__setBackend;

/**
 * Drains the microtask queue. Several passes, because the request is behind a
 * concurrency gate and then behind `getBackend()`, so one tick is not enough.
 */
const settle = () =>
  act(async () => {
    for (let i = 0; i < 20; i++) await Promise.resolve();
  });

beforeEach(async () => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
  vi.resetModules();
  asked = [];
  ({ __setBackend: setBackend } = await import("@/ipc/client"));
  // A backend that records the request and never answers: what is under test
  // is whether the call is made at all.
  setBackend({
    trackWaveform: (trackId: string) => {
      asked.push(trackId);
      return new Promise<Uint8Array>(() => {});
    },
  } as unknown as Backend);
  ({ WaveformPreview } = await import("./WaveformPreview"));
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  setBackend(null);
  vi.useRealTimers();
});

function mount(trackId: string) {
  act(() => root.render(<WaveformPreview trackId={trackId} width={120} height={20} />));
}

describe("WaveformPreview", () => {
  it("asks for nothing while the row is only being scrolled past", async () => {
    mount("1");
    // Two frames' worth: far less than anyone can read, and the case a flick
    // produces thousands of.
    void act(() => vi.advanceTimersByTime(33));
    void act(() => root.render(<></>));
    void act(() => vi.advanceTimersByTime(1000));
    await settle();
    expect(asked).toEqual([]);
  });

  it("asks once the row has settled", async () => {
    mount("2");
    void act(() => vi.advanceTimersByTime(200));
    await settle();
    expect(asked).toEqual(["2"]);
  });

  it("keeps a screenful of settled rows off the command channel at once", async () => {
    // Twenty rows land together when a scroll stops. Letting all twenty go
    // puts twenty file reads in front of the next `fetch_rows`.
    act(() => {
      root.render(
        <>
          {Array.from({ length: 20 }, (_, i) => (
            <WaveformPreview key={i} trackId={`row-${i}`} width={120} height={20} />
          ))}
        </>,
      );
    });
    void act(() => vi.advanceTimersByTime(200));
    await settle();
    // Some go, so this is a gate and not a mistake; not all twenty, which is
    // the point.
    expect(asked.length).toBeGreaterThan(0);
    expect(asked.length).toBeLessThanOrEqual(4);
  });
});
