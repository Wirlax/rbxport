import { describe, expect, it } from "vitest";

import { KEYMAP, KEYMAP_GROUPS, keyForPlatform } from "./keymap";
import { BINDINGS } from "./shortcuts";
import { rowsFor } from "@/views/settings/KeyboardPane";

describe("the Export key map", () => {
  it("has the ten groups the pane lists, in its order", () => {
    expect([...KEYMAP_GROUPS]).toEqual([
      "Browse", "Player A", "Player B", "General", "File", "View", "Track", "Playlist", "Help", "Link Export",
    ]);
  });

  it("holds every command the preset binds, and Player B is Player A with shift", () => {
    const bound = Object.values(KEYMAP).flat().filter((r) => r.key !== null);
    expect(bound).toHaveLength(124);
    for (const row of KEYMAP["Player B"]) {
      if (row.key === null) continue;
      expect(row.key.startsWith("shift + ")).toBe(true);
      const twin = KEYMAP["Player A"].find((a) => a.label === row.label);
      expect(twin, row.label).toBeDefined();
    }
  });

  it("opens on the capture's Player A rows in the capture's order", () => {
    const labels = KEYMAP["Player A"].map((r) => r.label);
    expect(labels.slice(0, 12)).toEqual([
      "Play/Pause", "Quantize", "Time Mode", "Cue", "Next Track", "Previous Track", "Memory Cue",
      "Loop In", "Loop Out", "Exit/Reloop", "1/64 Beat Loop", "1/32 Beat Loop",
    ]);
    expect(KEYMAP["Player A"].find((r) => r.label === "1 Beat Loop")?.key).toBe("4");
    expect(KEYMAP["Player A"].find((r) => r.label === "1/64 Beat Loop")?.key).toBeNull();
  });

  it("every binding that names a command names one that exists", () => {
    const ids = new Set(Object.values(KEYMAP).flat().map((r) => r.id));
    for (const binding of BINDINGS) {
      if (binding.command !== undefined) expect(ids.has(binding.command), binding.label).toBe(true);
      else expect(binding.pane, `${binding.label} needs a pane`).toBeDefined();
    }
  });

  it("prints command and option as ctrl and alt on Windows", () => {
    expect(keyForPlatform("shift + command + F", true)).toBe("shift + command + F");
    expect(keyForPlatform("shift + command + F", false)).toBe("shift + ctrl + F");
    expect(keyForPlatform("option + \\", false)).toBe("alt + \\");
  });

  it("the pane's rows are rekordbox's, live where built, plus this app's own", () => {
    const rows = rowsFor("Player A", true);
    expect(rows.find((r) => r.label === "Play/Pause")).toMatchObject({ key: "spacebar", built: true });
    expect(rows.find((r) => r.label === "Loop In")).toMatchObject({ key: "I", built: false });
    const browse = rowsFor("Browse", true);
    expect(browse.find((r) => r.label === "Search for tracks in the track list")?.built).toBe(true);
    expect(browse.find((r) => r.label === "Select All")).toMatchObject({ key: "command + A", built: true });
    expect(rowsFor("Help", false)).toEqual([]);
  });
});
