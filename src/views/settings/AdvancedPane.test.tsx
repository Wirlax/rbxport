/** @vitest-environment jsdom */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { __setBackend } from "@/ipc/client";
import type { Backend } from "@/ipc/types";
import { DEFAULT_PREFERENCES } from "@/lib/preferences";
import { PreferencesProvider } from "@/store/usePreferences";
import { AdvancedPane } from "./AdvancedPane";

let host: HTMLDivElement;
let root: Root;
let update: ReturnType<typeof vi.fn>;
let listBackups: ReturnType<typeof vi.fn>;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  update = vi.fn();
  listBackups = vi.fn().mockResolvedValue([]);
  __setBackend({ listBackups } as unknown as Backend);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  __setBackend(null);
});

function mount(protectLibrary = true) {
  const preferences = {
    ...DEFAULT_PREFERENCES,
    advanced: { ...DEFAULT_PREFERENCES.advanced, protectLibrary },
  };
  act(() => root.render(
    <PreferencesProvider value={{ preferences, update, reset: vi.fn() }}>
      <AdvancedPane tab="browse" summary={null} />
    </PreferencesProvider>,
  ));
}

async function toggleProtection() {
  const toggle = host.querySelector<HTMLInputElement>('input[role="switch"]');
  if (!toggle) throw new Error("Library Protection toggle missing");
  await act(async () => { toggle.click(); await Promise.resolve(); await Promise.resolve(); });
}

it("unlocks immediately when a backup exists", async () => {
  listBackups.mockResolvedValue([{ path: "/backups/library.zip" }]);
  mount();

  await toggleProtection();

  expect(update).toHaveBeenCalledWith("advanced", { protectLibrary: false });
  expect(host.querySelector('[role="dialog"]')).toBeNull();
});

it("gently recommends a backup and offers Unlock anyway or Cancel", async () => {
  mount();

  await toggleProtection();

  const dialog = host.querySelector('[role="dialog"]');
  expect(dialog?.textContent).toContain("It looks like you haven’t made a backup yet. We strongly recommend creating one before using RBXport.");
  expect([...dialog!.querySelectorAll("button")].map(button => button.textContent)).toEqual([
    "Unlock anyway", "Cancel",
  ]);
  expect(update).not.toHaveBeenCalled();
  await act(async () => { [...dialog!.querySelectorAll("button")][1]?.click(); await Promise.resolve(); });
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  expect(update).not.toHaveBeenCalled();
});

it("unlocks without a backup only after explicit confirmation", async () => {
  mount();

  await toggleProtection();
  const unlock = [...host.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')]
    .find(button => button.textContent === "Unlock anyway");
  await act(async () => { unlock?.click(); await Promise.resolve(); });

  expect(update).toHaveBeenCalledWith("advanced", { protectLibrary: false });
});

it("turns protection on without checking backups", async () => {
  mount(false);

  await toggleProtection();

  expect(update).toHaveBeenCalledWith("advanced", { protectLibrary: true });
  expect(listBackups).not.toHaveBeenCalled();
});

describe("Database management", () => {
  const local = { name: "Macintosh HD", masterDb: "/Users/x/Library/Pioneer/rekordbox/master.db", current: true };
  const drive = { name: "DJ SSD", masterDb: "/Volumes/DJ SSD/PIONEER/Master/master.db", current: false };
  const drives = [local, drive];

  async function mountDatabase(backend: Partial<Backend>, readOnly = false) {
    __setBackend({ listBackups, findDuplicates: vi.fn(), ...backend } as unknown as Backend);
    act(() => root.render(
      <PreferencesProvider value={{ preferences: DEFAULT_PREFERENCES, update, reset: vi.fn() }}>
        <AdvancedPane tab="database" summary={{ trackCount: 1, playlistCount: 1, dbVersion: "6000", readOnly } as never} />
      </PreferencesProvider>,
    ));
    await act(async () => { for (let i = 0; i < 4; i++) await Promise.resolve(); });
    return host.querySelector<HTMLSelectElement>('select[aria-label="Select a drive"]');
  }

  async function choose(select: HTMLSelectElement, value: string) {
    await act(async () => {
      select.value = value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      for (let i = 0; i < 4; i++) await Promise.resolve();
    });
  }

  it("lists the drives by name with the open library selected, as rekordbox does", async () => {
    const select = await mountDatabase({ databaseDrives: vi.fn().mockResolvedValue(drives) });
    expect(host.textContent).toContain("Database management");
    expect(host.textContent).toContain("Select a drive");
    expect([...select!.options].map((option) => option.textContent)).toEqual(["Macintosh HD", "DJ SSD"]);
    expect(select!.value).toBe(local.masterDb);
    expect(select!.disabled).toBe(false);
  });

  it("is greyed out with only one drive, and while rekordbox runs", async () => {
    expect((await mountDatabase({ databaseDrives: vi.fn().mockResolvedValue(drives.slice(0, 1)) }))!.disabled).toBe(true);
    expect((await mountDatabase({ databaseDrives: vi.fn().mockResolvedValue(drives) }, true))!.disabled).toBe(true);
  });

  it("asks before switching, and switches only on OK", async () => {
    const switchLibrary = vi.fn().mockResolvedValue(undefined);
    const confirm = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const select = await mountDatabase({ databaseDrives: vi.fn().mockResolvedValue(drives), confirm, switchLibrary });

    await choose(select!, drive.masterDb);
    expect(confirm).toHaveBeenCalledWith(
      "Are you sure you want to switch Master Database?\nThis operation may require long time.",
      { yes: "OK", no: "Cancel" },
    );
    expect(switchLibrary).not.toHaveBeenCalled();
    expect(select!.value).toBe(local.masterDb);

    await choose(select!, drive.masterDb);
    expect(switchLibrary).toHaveBeenCalledWith(drive.masterDb);
  });

  it("says rekordbox's words when the switch fails", async () => {
    const select = await mountDatabase({
      databaseDrives: vi.fn().mockResolvedValue(drives),
      confirm: vi.fn().mockResolvedValue(true),
      switchLibrary: vi.fn().mockRejectedValue({ kind: "internal", message: "Failed to switch Master Database." }),
    });
    await choose(select!, drive.masterDb);
    expect(host.textContent).toContain("Failed to switch Master Database.");
    expect(select!.value).toBe(local.masterDb);
  });
});

describe("Organize Library", () => {
  const folder = "/Users/x/Music Folder";
  const preview = { files: 3, tracks: 4, bytes: 3 * 1024 ** 2, inPlace: 1, missing: 2, leftAlone: 5 };

  async function mountOrganize(backend: Partial<Backend>, protectLibrary = false) {
    __setBackend({
      listBackups,
      findDuplicates: vi.fn(),
      databaseDrives: vi.fn().mockResolvedValue([]),
      lastOrganize: vi.fn().mockResolvedValue(null),
      onOrganizeProgress: vi.fn(() => () => {}),
      ...backend,
    } as unknown as Backend);
    const preferences = {
      ...DEFAULT_PREFERENCES,
      advanced: { ...DEFAULT_PREFERENCES.advanced, protectLibrary, musicFolder: folder },
    };
    act(() => root.render(
      <PreferencesProvider value={{ preferences, update, reset: vi.fn() }}>
        <AdvancedPane tab="database" summary={{ trackCount: 1, playlistCount: 1, dbVersion: "6000", readOnly: false } as never} />
      </PreferencesProvider>,
    ));
    await settle();
  }

  async function settle() {
    await act(async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); });
  }

  function button(label: string): HTMLButtonElement {
    const found = [...host.querySelectorAll("button")].find((b) => b.textContent === label);
    if (!found) throw new Error(`${label} missing`);
    return found;
  }

  async function press(label: string) {
    act(() => { button(label).click(); });
    await settle();
  }

  it("says what it would move, asks, backs up, then organizes", async () => {
    const calls: string[] = [];
    const confirm = vi.fn().mockResolvedValue(true);
    const backUpLibrary = vi.fn(() => { calls.push("backup"); return Promise.resolve("/backups/x"); });
    const organizeLibrary = vi.fn(() => { calls.push("organize"); return Promise.resolve({ files: 3, tracks: 4, failed: [] }); });
    await mountOrganize({ organizePreview: vi.fn().mockResolvedValue(preview), confirm, backUpLibrary, organizeLibrary });

    expect(host.textContent).toContain(folder);
    await press("Organize Library…");

    expect(confirm).toHaveBeenCalledWith([
      `Organize the library into ${folder}?`,
      "",
      "Files to move: 3 (3.0 MB)",
      "Already in place: 1",
      "Missing, not moved: 2",
      "Left alone (rekordbox’s own files, cloud and streaming): 5",
      "",
      "The library is backed up first, and this can be undone.",
    ].join("\n"), { yes: "Organize", no: "Cancel" });
    expect(calls).toEqual(["backup", "organize"]);
    expect(organizeLibrary).toHaveBeenCalledWith(folder);
    expect(host.textContent).toContain("Files moved: 3");
  });

  it("moves nothing when the answer is no", async () => {
    const organizeLibrary = vi.fn();
    const backUpLibrary = vi.fn();
    await mountOrganize({
      organizePreview: vi.fn().mockResolvedValue(preview), confirm: vi.fn().mockResolvedValue(false), backUpLibrary, organizeLibrary,
    });
    await press("Organize Library…");
    expect(backUpLibrary).not.toHaveBeenCalled();
    expect(organizeLibrary).not.toHaveBeenCalled();
  });

  it("is locked by Library Protection, and says so", async () => {
    await mountOrganize({}, true);
    expect(button("Organize Library…").disabled).toBe(true);
    expect(host.textContent).toContain("Editing is locked by Library Protection. Turn it off in Preferences to edit.");
  });

  it("puts the last run back after asking", async () => {
    const undoOrganize = vi.fn().mockResolvedValue({ files: 2, skipped: 0 });
    const confirm = vi.fn().mockResolvedValue(true);
    await mountOrganize({
      lastOrganize: vi.fn().mockResolvedValueOnce({ at: 0, files: 2 }).mockResolvedValue(null), confirm, undoOrganize,
    });
    await press("Undo Last Organize");
    expect(confirm.mock.calls[0]?.[0]).toContain("Put the 2 files moved on");
    expect(undoOrganize).toHaveBeenCalled();
    expect(host.textContent).toContain("Files put back: 2");
    expect(button("Undo Last Organize").disabled).toBe(true);
  });
});
