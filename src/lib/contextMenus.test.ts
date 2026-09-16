import { describe, expect, it } from "vitest";

import {
  deckMenu,
  enabled,
  entriesOf,
  SEPARATOR,
  TRACK_MENU,
  trackMenuFor,
  treeMenu,
  type MenuContext,
} from "./contextMenus";

const OPEN: MenuContext = { inPlaylist: true, hasFile: true, readOnly: false };

describe("TRACK_MENU", () => {
  it("is rekordbox's own list, in its own order, less the cloud", () => {
    // Transcribed from a capture of the menu open. If this drifts, the app has
    // stopped matching the thing it is a clone of — with the deliberate
    // exception of Cloud Library Sync, which is left out rather than greyed.
    expect(entriesOf(TRACK_MENU).map((e) => e.label)).toEqual([
      "Load",
      "Import To Collection",
      "Analyze Track",
      "Analysis Lock",
      "Add To Playlist",
      "Add To Tag List",
      "Reload Tag",
      "Get Info from iTunes",
      "Track Type",
      "Export Track",
      "Auto Load Hot Cue",
      "Reset DJ Play Count",
      "Add New Analysis Data",
      "Convert Memory Cues to Hot Cues",
      "Remove from Playlist",
      "Remove from Collection",
      "Remove from History",
      "Show information",
      "Show in Finder",
      "Track information",
    ]);
  });

  it("keeps the capture's seven separators", () => {
    expect(TRACK_MENU.filter((row) => row === SEPARATOR)).toHaveLength(7);
  });

  it("marks the entries that open a submenu", () => {
    const arrows = entriesOf(TRACK_MENU).filter((e) => e.submenu).map((e) => e.label);
    expect(arrows).toEqual([
      "Load",
      "Analysis Lock",
      "Add To Playlist",
      "Track Type",
      "Export Track",
      "Auto Load Hot Cue",
      "Track information",
    ]);
  });
});

describe("trackMenuFor", () => {
  const load = (players: number) =>
    entriesOf(trackMenuFor(players)).find((e) => e.label === "Load");

  it("offers the players the layout is drawing and no others", () => {
    expect(load(0)?.items).toBeUndefined();
    expect(entriesOf(load(1)?.items ?? []).map((e) => e.label)).toEqual([
      "Load track to player 1",
    ]);
    expect(entriesOf(load(2)?.items ?? []).map((e) => e.label)).toEqual([
      "Load track to player 1",
      "Load track to player 2",
    ]);
  });

  it("leaves the rest of rekordbox's list exactly as it was", () => {
    expect(entriesOf(trackMenuFor(2)).map((e) => e.label)).toEqual(
      entriesOf(TRACK_MENU).map((e) => e.label),
    );
    expect(trackMenuFor(2).filter((row) => row === SEPARATOR)).toHaveLength(7);
  });

  it("keeps Load greyed with no player to load into, and live with one", () => {
    // The arrow is drawn either way, which is what rekordbox does with an
    // entry a given selection cannot use.
    expect(load(0)?.submenu).toBe(true);
    expect(enabled(load(0) ?? { label: "Load", action: null }, OPEN)).toBe(false);
    expect(enabled(load(2) ?? { label: "Load", action: null }, OPEN)).toBe(true);
  });
});

describe("treeMenu", () => {
  it("is rekordbox's own list over a playlist, in its own order, less the cloud", () => {
    // docs/screenshots context-menu-tree@2x: thirteen entries in eight groups.
    // The three cloud rows shared the first group with Export Playlist, so
    // leaving them out takes it to ten; Rename, which the capture has no row
    // for, joins Delete's group and makes eleven in the same eight groups.
    expect(entriesOf(treeMenu("playlist")).map((e) => e.label)).toEqual([
      "Export Playlist",
      "Create New Playlist",
      "Create New Intelligent Playlist",
      "Create New Folder",
      "Playlist display setting",
      "Add Artwork",
      "Rename Playlist",
      "Delete Playlist",
      "Export a playlist to a file",
      "Collaborative playlist",
      "Add To Shortcut",
    ]);
    expect(treeMenu("playlist").filter((row) => row === SEPARATOR)).toHaveLength(7);
  });

  it("draws no cloud entry at all, rather than a greyed one", () => {
    const labels = entriesOf(treeMenu("playlist")).map((e) => e.label);
    expect(labels).not.toContain("Cloud Library Sync");
    expect(labels).not.toContain("Auto Upload");
    expect(labels).not.toContain("Batch Auto Upload setting");
  });

  it("marks the entries that open a submenu", () => {
    const arrows = entriesOf(treeMenu("playlist")).filter((e) => e.submenu).map((e) => e.label);
    expect(arrows).toEqual([
      "Export Playlist",
      "Export a playlist to a file",
      "Collaborative playlist",
    ]);
  });

  it("takes the node's own word for what is being deleted", () => {
    expect(entriesOf(treeMenu("folder")).map((e) => e.label)).toContain("Delete Folder");
    expect(entriesOf(treeMenu("playlist")).map((e) => e.label)).toContain("Delete Playlist");
    expect(entriesOf(treeMenu("folder")).map((e) => e.label)).toContain("Rename Folder");
    expect(entriesOf(treeMenu("playlist")).map((e) => e.label)).toContain("Rename Playlist");
    expect(entriesOf(treeMenu("folder")).map((e) => e.label)).toContain("Export Folder");
    expect(entriesOf(treeMenu("playlist")).map((e) => e.label)).toContain("Export Playlist");
  });
});

describe("enabled", () => {
  const entry = (label: string) =>
    entriesOf(TRACK_MENU).find((e) => e.label === label) ?? { label, action: null };

  it("greys what rekordbox has and this does not", () => {
    expect(enabled(entry("Load"), OPEN)).toBe(false);
    expect(enabled(entry("Get Info from iTunes"), OPEN)).toBe(false);
  });

  it("greys Analyze Track: the engine is there, the result is not offered", () => {
    expect(enabled(entry("Analyze Track"), OPEN)).toBe(false);
  });

  it("draws no cloud entry in the track menu either", () => {
    expect(entriesOf(TRACK_MENU).map((e) => e.label)).not.toContain("Cloud Library Sync");
  });

  it("greys removing from a playlist when the view is not one", () => {
    expect(enabled(entry("Remove from Playlist"), { ...OPEN, inPlaylist: false })).toBe(false);
    expect(enabled(entry("Remove from Playlist"), OPEN)).toBe(true);
  });

  it("greys showing a file that is not there", () => {
    expect(enabled(entry("Show in Finder"), { ...OPEN, hasFile: false })).toBe(false);
  });

  it("greys every write while rekordbox holds the library", () => {
    // The same rule the rest of the app follows: a refusal, not a race.
    const locked = { ...OPEN, readOnly: true };
    expect(enabled(entry("Remove from Playlist"), locked)).toBe(false);
    // Reading is still fine.
    expect(enabled(entry("Show information"), locked)).toBe(true);
  });
});

describe("deckMenu", () => {
  const state = { waveformColor: "3band" as const, beatCount: "position" as const, waveformClick: true };

  it("is rekordbox's player menu, top to bottom, with its greyed entries", () => {
    const labels = entriesOf(deckMenu(state)).map((e) => e.label);
    expect(labels).toEqual([
      "Change waveform color", "Analyze Track", "Beat Count Display", "Export Track", "Export Loop As WAV",
      "Active Loop Playback", "Click on the waveform for PLAY and CUE",
    ]);
    const context = { inPlaylist: false, hasFile: true, readOnly: false };
    const live = entriesOf(deckMenu(state)).filter((e) => enabled(e, context)).map((e) => e.label);
    expect(live).toEqual(["Change waveform color", "Beat Count Display", "Click on the waveform for PLAY and CUE"]);
  });

  it("ticks the choice in force in each submenu", () => {
    const rows = deckMenu({ ...state, waveformColor: "rgb", beatCount: "toMemoryBeats", waveformClick: false });
    const items = (label: string) => entriesOf(rows).find((e) => e.label === label)?.items ?? [];
    expect(entriesOf(items("Change waveform color")).map((e) => [e.label, e.checked ?? false]))
      .toEqual([["BLUE", false], ["RGB", true], ["3Band", false]]);
    expect(entriesOf(items("Beat Count Display")).filter((e) => e.checked).map((e) => e.label))
      .toEqual(["Count to the next MEMORY CUE (Beats)"]);
    expect(entriesOf(items("Click on the waveform for PLAY and CUE")).filter((e) => e.checked).map((e) => e.label))
      .toEqual(["Disable"]);
  });

  it("greys Analyze Track, loaded or not: the engine is there, the result is not offered", () => {
    const entry = entriesOf(deckMenu(state)).find((e) => e.label === "Analyze Track");
    expect(entry?.action).toBeNull();
  });
});
