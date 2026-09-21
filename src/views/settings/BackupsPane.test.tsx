// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BackupsPane } from "./BackupsPane";

const held = vi.hoisted(() => ({
  preferences: { advanced: { protectLibrary: false } },
  backend: { backupSizes: vi.fn(), startBackup: vi.fn(), cancelBackup: vi.fn(), backupProgress: vi.fn(), backupDirectory: vi.fn(), listBackups: vi.fn(), backUpLibrary: vi.fn(), restoreBackup: vi.fn(), deleteBackup: vi.fn(), confirm: vi.fn() },
}));
vi.mock("@/ipc/client", () => ({ getBackend: () => Promise.resolve(held.backend) }));
vi.mock("@/store/usePreferences", () => ({ usePreferences: () => held.preferences }));
let host: HTMLDivElement;
let root: Root;
const entry = { path: "/backups/library-test", name: "library-test", bytes: 1048576, createdAt: 1700000000000, includesAnalysis: true };
const button = (name: string) => [...host.querySelectorAll("button")].find(b => b.textContent === name)!;
beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  vi.resetAllMocks();
  held.preferences.advanced.protectLibrary = false;
  held.backend.listBackups.mockResolvedValue([entry]);
  held.backend.backupDirectory.mockResolvedValue("/backups");
  held.backend.confirm.mockResolvedValue(false);
  held.backend.backupSizes.mockResolvedValue({ updatedAt: 1700000000000, database: 1024, waveforms: 4096, cues: 256, beatGrids: 512, phrases: 128, other: 128 });
  held.backend.backupProgress.mockResolvedValue({ running: false, phase: "", copiedBytes: 0, totalBytes: 0, path: null, error: null });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(() => { act(() => root.unmount()); host.remove(); vi.useRealTimers(); });
it("cancelling restore or delete leaves the backup and library untouched", async () => {
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  await act(async () => { button("Restore").click(); await Promise.resolve(); });
  await act(async () => { button("Delete").click(); await Promise.resolve(); });
  expect(held.backend.confirm).toHaveBeenCalledTimes(2);
  expect(held.backend.restoreBackup).not.toHaveBeenCalled();
  expect(held.backend.deleteBackup).not.toHaveBeenCalled();
  expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
});
it("library protection disables restore, but allows creating and deleting backups", async () => {
  held.preferences.advanced.protectLibrary = true;
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  expect(button("Restore").disabled).toBe(true);
  expect(button("Create backup").disabled).toBe(false);
  expect(button("Delete").disabled).toBe(false);
});
it("shows a restore error and re-enables the controls", async () => {
  held.backend.confirm.mockResolvedValue(true);
  held.backend.restoreBackup.mockRejectedValue(new Error("Backup is damaged"));
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  await act(async () => { button("Restore").click(); await Promise.resolve(); });
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("Backup is damaged");
  expect(button("Restore").disabled).toBe(false);
});
it("reconnects to a background job after reopening Preferences and can stop it", async () => {
  vi.useFakeTimers();
  const progress = { running: true, phase: "copying", copiedBytes: 50, totalBytes: 100, path: null, error: null };
  held.backend.startBackup.mockImplementation(() => {
    held.backend.backupProgress.mockResolvedValue(progress);
    return Promise.resolve();
  });
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  await act(async () => { button("Create backup").click(); await Promise.resolve(); });
  expect(button("Create backup")).toBeUndefined();
  expect(button("Restore").disabled).toBe(true);
  expect(host.querySelector('[role="status"]')?.textContent).toContain("50%");
  act(() => root.unmount());
  root = createRoot(host);
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  expect(host.querySelector('[role="status"]')?.textContent).toContain("50%");
  held.backend.cancelBackup.mockImplementation(() => {
    held.backend.backupProgress.mockResolvedValue({ ...progress, phase: "stopping" });
    return Promise.resolve();
  });
  await act(async () => { button("Stop backup").click(); await Promise.resolve(); });
  expect(button("Stopping…").disabled).toBe(true);
  held.backend.backupProgress.mockResolvedValue({ ...progress, running: false, phase: "cancelled" });
  await act(async () => { await vi.advanceTimersByTimeAsync(500); });
  expect(host.querySelector('[role="status"]')?.textContent).toBe("Backup stopped.");
  expect(button("Create backup").disabled).toBe(false);
  expect(held.backend.startBackup).toHaveBeenCalledTimes(1);
  expect(held.backend.cancelBackup).toHaveBeenCalledTimes(1);
});
it("refreshes the saved backups when a background job completes", async () => {
  vi.useFakeTimers();
  held.backend.listBackups.mockResolvedValue([]);
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  held.backend.listBackups.mockResolvedValue([entry]);
  held.backend.backupProgress.mockResolvedValue({ running: false, phase: "complete", copiedBytes: 100, totalBytes: 100, path: entry.path, error: null });
  await act(async () => { await vi.advanceTimersByTimeAsync(500); });
  expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
  expect(host.querySelector('[role="status"]')?.textContent).toBe("Backup created.");
  expect(held.backend.backupSizes).toHaveBeenCalledTimes(1);
});
it("shows the empty graph, legend and spinner during the initial calculation", async () => {
  held.backend.backupSizes.mockReturnValue(new Promise(() => {}));
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  const bar = host.querySelector('[role="img"]');
  expect(bar?.children).toHaveLength(0);
  expect(bar?.parentElement?.getAttribute("aria-busy")).toBe("true");
  expect(bar?.parentElement?.querySelector('[role="status"]')?.textContent).toBe("Calculating Rekordbox data size");
  expect(bar?.parentElement?.querySelector("svg")).not.toBeNull();
  expect(host.querySelector('[aria-label="Backup size breakdown"]')?.children).toHaveLength(6);
  expect(host.textContent).toContain("Last updated: —");
  expect(button("Refresh").disabled).toBe(true);
  expect(held.backend.backupSizes).toHaveBeenCalledWith(false);
});
it("refreshes explicitly and keeps the last successful reading on failure", async () => {
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  const previousTime = host.querySelector("time")?.dateTime;
  held.backend.backupSizes.mockRejectedValueOnce(new Error("unavailable"));
  await act(async () => { button("Refresh").click(); await Promise.resolve(); });
  expect(held.backend.backupSizes).toHaveBeenLastCalledWith(true);
  expect(host.querySelector("time")?.dateTime).toBe(previousTime);
  expect(host.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain("Waveform previews: 4.0 KB");
  held.backend.backupSizes.mockResolvedValue({ updatedAt: 1800000000000, database: 2048, waveforms: 0, cues: 0, beatGrids: 0, phrases: 0, other: 0 });
  await act(async () => { button("Refresh").click(); await Promise.resolve(); });
  expect(host.querySelector("time")?.dateTime).toBe(new Date(1800000000000).toISOString());
  expect(host.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain("Database: 2.0 KB");
  expect(host.querySelector('[role="alert"]')).toBeNull();
});
it("keeps backup actions usable if sizes cannot be measured, and lets the user retry", async () => {
  held.backend.backupSizes.mockRejectedValueOnce(new Error("unavailable"));
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  expect(button("Create backup").disabled).toBe(false);
  expect(host.textContent).toContain("Couldn’t calculate Rekordbox data size.");
  await act(async () => { button("Refresh").click(); await Promise.resolve(); });
  expect(host.querySelector('[aria-label="Backup size breakdown"]')?.children).toHaveLength(6);
  expect(host.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain("Waveform previews: 4.0 KB");
});
it("shows an empty size bar without invalid segment widths when there is no data", async () => {
  held.backend.backupSizes.mockResolvedValue({ updatedAt: 1700000000000, database: 0, waveforms: 0, cues: 0, beatGrids: 0, phrases: 0, other: 0 });
  await act(async () => { root.render(<BackupsPane />); await Promise.resolve(); });
  const bar = host.querySelector('[role="img"]');
  expect(bar?.getAttribute("aria-label")).toBe("No data to back up");
  expect(bar?.children).toHaveLength(0);
});
