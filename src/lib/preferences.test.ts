import { describe, expect, it } from "vitest";

import {
  browseScale,
  BROWSE_SCALE_DEFAULT,
  DEFAULT_PREFERENCES,
  formatKey,
  quantizeFraction,
  sanitisePreferences,
} from "./preferences";

describe("sanitisePreferences", () => {
  it("gives the defaults for nothing, garbage, and a wrong shape", () => {
    expect(sanitisePreferences(undefined)).toEqual(DEFAULT_PREFERENCES);
    expect(sanitisePreferences("view")).toEqual(DEFAULT_PREFERENCES);
    expect(sanitisePreferences({ view: 3, advanced: [] })).toEqual(DEFAULT_PREFERENCES);
  });

  it("keeps every valid choice and replaces each bad one on its own", () => {
    const stored = {
      view: {
        tooltips: false,
        browseFontSize: 4,
        browseLineSpace: 9,
        keyDisplay: "alphanumeric",
        waveformRate: "fast",
        overviewWaveform: "full",
        explorer: "yes",
      },
      analysis: { auto: false },
      djSystem: { waveformColor: "rgb", subColumn: 5.5, categories: [{ id: 1 }], linkInterface: "" },
      advanced: {
        relocateFolders: ["/a", "", 3, "/b"],
        syncType: "bpm",
        quantizeBeat: "1/16",
        protectLibrary: true,
      },
    };
    const out = sanitisePreferences(stored);
    expect(out.view.tooltips).toBe(false);
    expect(out.view.browseFontSize).toBe(4);
    expect(out.view.browseLineSpace).toBe(BROWSE_SCALE_DEFAULT);
    expect(out.view.keyDisplay).toBe("alphanumeric");
    expect(out.view.waveformRate).toBe("high");
    expect(out.view.overviewWaveform).toBe("full");
    expect(out.view.explorer).toBe(true);
    expect(out.analysis.auto).toBe(false);
    expect(out.djSystem.waveformColor).toBe("rgb");
    expect(out.djSystem.subColumn).toBeNull();
    // A row that is not a row means the reference rows, not a broken list.
    expect(out.djSystem.categories).toBeNull();
    // An empty interface name is no choice: LINK picks for itself.
    expect(out.djSystem.linkInterface).toBeNull();
    expect(sanitisePreferences({ djSystem: { linkInterface: "en11" } }).djSystem.linkInterface).toBe("en11");
    expect(out.advanced.relocateFolders).toEqual(["/a", "/b"]);
    expect(out.advanced.syncType).toBe("bpm");
    expect(out.advanced.quantizeBeat).toBe("1/1");
    expect(out.advanced.protectLibrary).toBe(true);
  });

  it("checks for updates unless the store plainly says not to", () => {
    // A store written before the switch existed has no such key.
    expect(sanitisePreferences({ advanced: {} }).advanced.checkUpdates).toBe(true);
    expect(sanitisePreferences({ advanced: { checkUpdates: "no" } }).advanced.checkUpdates).toBe(true);
    expect(sanitisePreferences({ advanced: { checkUpdates: 0 } }).advanced.checkUpdates).toBe(true);
    expect(sanitisePreferences({ advanced: { checkUpdates: false } }).advanced.checkUpdates).toBe(false);
  });

  it("keeps stored rows when every one is a row", () => {
    const rows = [{ id: 1, menuItem: 1, name: "GENRE", seq: 1, visible: true }];
    expect(sanitisePreferences({ djSystem: { categories: rows } }).djSystem.categories).toEqual(rows);
  });
});

describe("formatKey", () => {
  it("shows the library's name, or the Camelot code when asked", () => {
    expect(formatKey("Ebm", "classic")).toBe("Ebm");
    expect(formatKey("Ebm", "alphanumeric")).toBe("2A");
    expect(formatKey("C", "alphanumeric")).toBe("8B");
    // A key the wheel does not know is shown as it is, not hidden.
    expect(formatKey("Unknown", "alphanumeric")).toBe("Unknown");
    expect(formatKey("", "alphanumeric")).toBe("");
  });
});

describe("the sliders and the quantize value", () => {
  it("scale from the measured size in the middle", () => {
    expect(browseScale(BROWSE_SCALE_DEFAULT)).toBe(1);
    expect(browseScale(0)).toBeLessThan(1);
    expect(browseScale(4)).toBeGreaterThan(1);
    expect(browseScale(99)).toBe(1);
  });

  it("turn a beat value into a fraction of a beat", () => {
    expect(quantizeFraction("1/1")).toBe(1);
    expect(quantizeFraction("1/2")).toBe(0.5);
    expect(quantizeFraction("1/8")).toBe(0.125);
  });
});
