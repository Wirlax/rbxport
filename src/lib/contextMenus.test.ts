import { describe, expect, it } from "vitest";

import {
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
  it("is rekordbox's own list, in its own order", () => {
    // Transcribed from a capture of the menu open. If this drifts, the app has
    // stopped matching the thing it is a clone of.
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
      "Cloud Library Sync",
      "Export Track",
      "Auto Load Hot Cue",
      "Reset DJ Play Count",
      "Add New Analysis Data",
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
      "Cloud Library Sync",
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
  it("takes the node's own word for what is being deleted", () => {
    expect(entriesOf(treeMenu("folder")).map((e) => e.label)).toContain("Delete Folder");
    expect(entriesOf(treeMenu("playlist")).map((e) => e.label)).toContain("Delete Playlist");
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
    expect(enabled(entry("Analyze Track"), OPEN)).toBe(true);
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
    expect(enabled(entry("Analyze Track"), locked)).toBe(true);
  });
});
