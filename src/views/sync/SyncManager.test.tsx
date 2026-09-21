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
import { PreferencesProvider } from "@/store/usePreferences";
import { DEFAULT_PREFERENCES } from "@/lib/preferences";

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
  volumeId: "dev:1",
  export: null,
});
const DEVICES = [stick("USB A"), stick("USB B")];

const STATES: Record<string, DeviceSyncState> = {
  "/Volumes/USB A": { selected: [{ libraryId: "p3", name: "Closing" }], onDevice: ["Closing"], automatic: true },
  "/Volumes/USB B": { selected: [], onDevice: [], automatic: false },
};

const report = (path: string, tracks: number): SyncDeviceReport => ({
  path,
  report: { tracks, playlists: 1, bytesCopied: 0, analysisFiles: 0, reused: 0, removed: 0, skipped: [], verified: true },
});

let host: HTMLDivElement;
let root: Root;
let importUsb: ReturnType<typeof vi.fn>;
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
  importUsb = vi.fn(() => Promise.resolve({ tracks: 2, histories: 0, settings: 0, skipped: 0 }));
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
    importUsb,
    confirm: () => Promise.resolve(true),
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
  it("passes cleanup and the chosen compatibility format to sync", async () => {
    const preferences = { ...DEFAULT_PREFERENCES, usbExport: {
      ...DEFAULT_PREFERENCES.usbExport, deleteUnlistedMusic: true,
      maximumCompatibility: true, conversionFormat: "mp3" as const,
    } };
    act(() => root.render(<PreferencesProvider value={{ preferences, update: vi.fn(), reset: vi.fn() }}>
      <SyncManager onClose={onClose} />
    </PreferencesProvider>));
    await settle();
    click(box("Sets"));
    click(box("USB A"));
    await settle();
    click(host.querySelector<HTMLButtonElement>('button[aria-label="SYNC"]'));
    await settle();
    expect(syncDevices.mock.calls[0]?.slice(5)).toEqual([true, "mp3"]);
  });

  it("lists the playlists and folders, not All Tracks or the heading", () => {
    const names = [...host.querySelectorAll('[aria-label="Playlists"] > [role="treeitem"]')].map((row) => row.textContent?.trim());
    expect(names).toEqual(["Sets", "Warm Up", "Main Set", "Closing"]);
    expect(host.querySelector('button[aria-label="SYNC"]')).toHaveProperty("disabled", true);
  });

  it("shows used space as the filled portion and labels the remaining free space", () => {
    const meter = host.querySelector('[role="meter"][aria-label="USB A storage used"]');
    expect(meter?.getAttribute("aria-valuenow")).toBe("25");
    expect(meter?.getAttribute("aria-valuetext")).toBe("8.0 GB used; 24.0 GB free (75%)");
    expect(meter?.querySelector("span")?.style.width).toBe("25%");
  });

  it("requests post-sync ejection and distinguishes eject errors from sync errors", async () => {
    syncDevices.mockResolvedValueOnce([
      { ...report("/Volumes/USB A", 5), ejected: true },
      { ...report("/Volumes/USB B", 5), ejectError: "Device is busy." },
    ]);
    expect(box("Eject after syncing")?.checked).toBe(false);
    click(box("USB A"));
    click(box("USB B"));
    click(box("Eject after syncing"));
    await settle();
    click(host.querySelector<HTMLButtonElement>('button[aria-label="SYNC"]'));
    expect(box("Eject after syncing")?.disabled).toBe(true);
    await settle();
    expect(syncDevices.mock.calls[0]?.[4]).toBe(true);
    expect(status()).toContain("Exported 5 tracks to USB A");
    expect(status()).toContain("Safely ejected.");
    expect(status()).toContain("Exported 5 tracks to USB B");
    expect(status()).toContain("Not ejected: Device is busy.");
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

  it("ticking a device restores its selection without expanding it", async () => {
    expect(box("Closing")?.checked).toBe(false);
    click(box("USB A"));
    await settle();
    expect(box("Closing")?.checked).toBe(true);
    expect(host.querySelector('[aria-label="USB A library"]')).toBeNull();
    expect(box("USB A")?.closest('[role="treeitem"]')?.getAttribute("aria-expanded")).toBe("false");
    click(host.querySelector<HTMLButtonElement>('button[aria-label="Expand USB A"]'));
    await settle();
    const library = host.querySelector('[aria-label="USB A library"]');
    expect(library?.textContent).toContain("Device Library");
    expect(library?.textContent).toContain("Closing");
    expect(host.textContent).toContain("24.0 GB free (75%)");
    // A stick with nothing on it says so, and ticks nothing.
    click(box("USB B"));
    await settle();
    expect(host.querySelector('[aria-label="USB B library"]')).toBeNull();
    click(host.querySelector<HTMLButtonElement>('button[aria-label="Expand USB B"]'));
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
    const [playlists, destinations, , automatic] = syncDevices.mock.calls[0] as [string[], string[], unknown, boolean];
    expect(playlists).toEqual(["p1", "p2", "p3"]);
    expect(destinations).toEqual(["/Volumes/USB A", "/Volumes/USB B"]);
    expect(automatic).toBe(false);
    expect(syncDevices.mock.calls[0]?.[4]).toBe(false);
    expect(box("Automatic synchronization for USB A")).toBeNull();
    expect(box("Automatic synchronization for USB B")).toBeNull();
    expect(status()).toContain("Exported 30 tracks to USB A");
    expect(status()).toContain("Exported 30 tracks to USB B");
    expect(sync?.disabled).toBe(false);
  });

  it("a stick that failed says why, beside the ones that were written", async () => {
    syncDevices.mockImplementationOnce(() => Promise.resolve([
      report("/Volumes/USB A", 5),
      { path: "/Volumes/USB B", error: "That device is no longer connected." },
    ]));
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

it("imports cue/grid information from selected devices only", async () => {
  const button = [...host.querySelectorAll("button")].find(b => b.textContent?.includes("CUE GRID INFO"))!;
  expect(button.disabled).toBe(true);
  click(box("USB A"));
  await settle();
  click(button);
  await settle();
  expect(importUsb).toHaveBeenCalledWith("/Volumes/USB A", true, false, false);
  expect(importUsb).toHaveBeenCalledTimes(1);
  expect(status()).toContain("updated 2 tracks");
});

it("expanding a USB does not select it for synchronization", async () => {
  click(host.querySelector<HTMLButtonElement>('button[aria-label="Expand USB A"]'));
  await settle();
  expect(box("USB A")?.checked).toBe(false);
  expect(host.querySelector('[aria-label="USB A library"]')?.textContent).toContain("Closing");
});
