/**
 * @vitest-environment jsdom
 *
 * The Updates section in About: the switch writes the
 * preference, and the button asks the main window to check — through the
 * backend, because this pane may be a window of its own.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { __setBackend } from "@/ipc/client";
import type { Backend, PreferencesRequest } from "@/ipc/types";
import { DEFAULT_PREFERENCES, type Preferences } from "@/lib/preferences";
import { PreferencesProvider, type PreferencesStore } from "@/store/usePreferences";
import { AboutPane } from "./AboutPane";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let host: HTMLDivElement;
let root: Root;
let update: ReturnType<typeof vi.fn>;
let requested: PreferencesRequest[];

function mount(preferences: Preferences = DEFAULT_PREFERENCES) {
  const store = { preferences, update, reset: vi.fn() } as unknown as PreferencesStore;
  act(() => {
    root.render(
      <PreferencesProvider value={store}>
        <AboutPane />
      </PreferencesProvider>,
    );
  });
}

const settle = () =>
  act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });

function section(title = "About"): Element {
  const found = host.querySelector(`section[aria-label="${title}"]`);
  if (!found) throw new Error(`no ${title} section`);
  return found;
}

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  update = vi.fn();
  requested = [];
  __setBackend({
    appVersion: () => Promise.resolve("0.4.0"),
    requestPreferencesReset: (what: PreferencesRequest) => {
      requested.push(what);
      return Promise.resolve();
    },
  } as unknown as Backend);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  __setBackend(null);
});

describe("AboutPane › Updates", () => {
  it("the switch shows the preference and writes its opposite", async () => {
    mount();
    await settle();
    const toggle = section().querySelector<HTMLInputElement>('input[role="switch"]');
    expect(toggle?.checked).toBe(true);
    act(() => toggle?.click());
    expect(update).toHaveBeenCalledWith("advanced", { checkUpdates: false });
    expect(update).toHaveBeenCalledTimes(1);
  });

  it("the switch reads a stored no as off and writes yes back", async () => {
    mount({ ...DEFAULT_PREFERENCES, advanced: { ...DEFAULT_PREFERENCES.advanced, checkUpdates: false } });
    await settle();
    const toggle = section().querySelector<HTMLInputElement>('input[role="switch"]');
    expect(toggle?.checked).toBe(false);
    act(() => toggle?.click());
    expect(update).toHaveBeenCalledWith("advanced", { checkUpdates: true });
  });

  it("the button asks the main window for a check rather than checking here", async () => {
    mount();
    const button = Array.from(section().querySelectorAll("button")).find(
      (b) => b.textContent === "Check for updates",
    );
    if (!button) throw new Error("no Check for Updates button");
    act(() => button.click());
    await settle();
    expect(requested).toEqual(["updates"]);
    expect(update).not.toHaveBeenCalled();
  });
});
