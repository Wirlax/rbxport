/**
 * @vitest-environment jsdom
 *
 * The Sync Manager's ticking and its SYNC: a folder ticks what is under it
 * and shows part-way when only some of that is ticked, a ticked stick brings
 * its last selection back, and SYNC sends the ticked playlists to every
 * ticked stick and reports on each.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { __setBackend } from "@/ipc/client";
import type { Backend, Device, DeviceSyncState, SyncDeviceReport, SyncProgress, TreeNode } from "@/ipc/types";
import { SyncManager } from "./SyncManager";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

const TREE: TreeNode[] = [
  { id: "all", name: "All Tracks", kind: "allTracks", depth: 0 },
  { id: "playlists", name: "Playlists", kind: "collection", depth: 0, expanded: true },
  { id: "f1", name: "Sets", kind: "folder", depth: 1, expanded: true },
  { id: "p1", name: "Warm Up", kind: "playlist", depth: 2 },
  { id: "p2", name: "Main Set", kind: "playlist", depth: 2 },
  { id: "p3", name: "Closing", kind: "playlist", depth: 1 },
  { id: "histories", name: "Histories", kind: "histories", depth: 0 },
];

const stick = (name: string): Device => ({
  name,
  path: `/Volumes/${name}`,
  totalBytes: 32 * 1024 ** 3,
  freeBytes: 24 * 1024 ** 3,
  removable: true,
  export: null,
});
const DEVICES = [stick("USB A"), stick("USB B")];

const STATES: Record<string, DeviceSyncState> = {
  "/Volumes/USB A": { selected: [{ libraryId: "p3", name: "Closing" }], onDevice: ["Closing"] },
  "/Volumes/USB B": { selected: [], onDevice: [] },
};

const report = (path: string, tracks: number): SyncDeviceReport => ({
  path,
  report: { tracks, playlists: 1, bytesCopied: 0, analysisFiles: 0, reused: 0, removed: 0, skipped: [], verified: true },
});

let host: HTMLDivElement;
let root: Root;
let syncDevices: ReturnType<typeof vi.fn>;
let progress: ((p: SyncProgress) => void) | null;
let onClose: ReturnType<typeof vi.fn>;

const settle = () =>
  act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });

const box = (label: string) => host.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
const click = (el: HTMLElement | null | undefined) => {
  if (!el) throw new Error("nothing to click");
  act(() => el.click());
};
const status = () => host.querySelector('[role="status"]')?.textContent ?? "";

beforeEach(async () => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  progress = null;
  onClose = vi.fn();
  syncDevices = vi.fn((playlists: string[], destinations: string[]) =>
    Promise.resolve(destinations.map((path) => report(path, playlists.length * 10))),
  );
  __setBackend({
    playlistTree: () => Promise.resolve(TREE.map((n) => ({ ...n }))),
    listDevices: () => Promise.resolve(DEVICES.map((d) => ({ ...d }))),
    deviceSyncState: (path: string) => {
      const state = STATES[path];
      return state ? Promise.resolve(state) : Promise.reject(new Error("gone"));
    },
    syncDevices,
    onSyncProgress: (listener: (p: SyncProgress) => void) => {
      progress = listener;
      return () => {
        progress = null;
      };
    },
  } as unknown as Backend);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root.render(<SyncManager onClose={onClose} />);
  });
  await settle();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  __setBackend(null);
});

describe("SyncManager", () => {
  it("lists the playlists and folders, not All Tracks or the heading", () => {
    const names = [...host.querySelectorAll('[role="treeitem"]')].map((row) => row.textContent?.trim());
    expect(names).toEqual(["Sets", "Warm Up", "Main Set", "Closing"]);
    expect(host.querySelector('button[aria-label="SYNC"]')).toHaveProperty("disabled", true);
  });

  it("ticking a folder ticks every playlist under it, and part of it shows as mixed", () => {
    click(box("Sets"));
    expect(box("Warm Up")?.checked).toBe(true);
    expect(box("Main Set")?.checked).toBe(true);
    expect(box("Closing")?.checked).toBe(false);
    expect(box("Sets")?.getAttribute("aria-checked")).toBe("true");

    click(box("Warm Up"));
    expect(box("Sets")?.checked).toBe(false);
    expect(box("Sets")?.indeterminate).toBe(true);
    expect(box("Sets")?.getAttribute("aria-checked")).toBe("mixed");

    // A mixed folder ticks the rest on a click, as a native tri-state box
    // does; the click after that clears it.
    click(box("Sets"));
    expect(box("Warm Up")?.checked).toBe(true);
    expect(box("Sets")?.indeterminate).toBe(false);
    click(box("Sets"));
    expect(box("Warm Up")?.checked).toBe(false);
    expect(box("Main Set")?.checked).toBe(false);
  });

  it("ticking a device ticks what it was last synced with and shows what it holds", async () => {
    expect(box("Closing")?.checked).toBe(false);
    click(box("USB A"));
    await settle();
    expect(box("Closing")?.checked).toBe(true);
    const library = host.querySelector('[aria-label="USB A library"]');
    expect(library?.textContent).toContain("Device Library");
    expect(library?.textContent).toContain("Closing");
    expect(library?.textContent).toContain("24.0 GB free of 32.0 GB");
    // A stick with nothing on it says so, and ticks nothing.
    click(box("USB B"));
    await settle();
    expect(host.querySelector('[aria-label="USB B library"]')?.textContent).toContain("No playlists on this device yet.");
    expect(box("Warm Up")?.checked).toBe(false);
  });

  it("SYNC sends the ticked playlists to both ticked sticks and reports on each", async () => {
    // The write is held open so the line it shows while running can be read.
    let finish: (reports: SyncDeviceReport[]) => void = () => {};
    syncDevices.mockImplementationOnce(
      () =>
        new Promise<SyncDeviceReport[]>((resolve) => {
          finish = resolve;
        }),
    );
    click(box("Sets"));
    click(box("USB A"));
    click(box("USB B"));
    await settle();
    const sync = host.querySelector<HTMLButtonElement>('button[aria-label="SYNC"]');
    expect(sync?.disabled).toBe(false);
    click(sync);
    await settle();
    expect(sync?.disabled).toBe(true);
    // The run is announced stick by stick while it is going.
    act(() => progress?.({ path: "/Volumes/USB A", state: "writing" }));
    expect(status()).toBe("Writing to USB A…");
    act(() => finish([report("/Volumes/USB A", 30), report("/Volumes/USB B", 30)]));
    await settle();
    expect(syncDevices).toHaveBeenCalledTimes(1);
    const [playlists, destinations] = syncDevices.mock.calls[0] as [string[], string[]];
    // In tree order, the folder's two and the one USB A brought back.
    expect(playlists).toEqual(["p1", "p2", "p3"]);
    expect(destinations).toEqual(["/Volumes/USB A", "/Volumes/USB B"]);
    expect(status()).toContain("Exported 30 tracks to USB A");
    expect(status()).toContain("Exported 30 tracks to USB B");
    expect(sync?.disabled).toBe(false);
  });

  it("a stick that failed says why, beside the ones that were written", async () => {
    syncDevices.mockImplementationOnce((_playlists: string[], destinations: string[]) =>
      Promise.resolve([
        report(destinations[0] ?? "", 5),
        { path: destinations[1] ?? "", error: "That device is no longer connected." },
      ]),
    );
    click(box("Closing"));
    click(box("USB A"));
    click(box("USB B"));
    await settle();
    click(host.querySelector<HTMLButtonElement>('button[aria-label="SYNC"]'));
    await settle();
    expect(status()).toContain("Exported 5 tracks to USB A");
    expect(status()).toContain("USB B: That device is no longer connected.");
  });

  it("Close and Escape both close it", () => {
    click([...host.querySelectorAll("button")].find((b) => b.textContent === "Close"));
    expect(onClose).toHaveBeenCalledTimes(1);
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
