/** @vitest-environment jsdom */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

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
