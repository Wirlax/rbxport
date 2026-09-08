import { describe, expect, it } from "vitest";

import { DEFAULT_SESSION, DEFAULT_TREE_WIDTH, sanitiseSession } from "./session";
import { DEFAULT_SORT } from "./viewSpec";

describe("sanitiseSession", () => {
  it("accepts a session it wrote itself", () => {
    const session = {
      treeWidth: 420,
      selectedNodeId: "pl-7",
      sort: { column: "bpm", descending: true },
      infoOpen: true,
      subOpen: false,
    };
    expect(sanitiseSession(session)).toEqual(session);
  });

  it("falls back on anything that is not a session", () => {
    // A working window matters more than honouring whatever was stored.
    for (const bad of [null, undefined, 7, "x", [], true]) {
      expect(sanitiseSession(bad)).toEqual(DEFAULT_SESSION);
    }
  });

  it("rejects a width that would collapse or explode the tree", () => {
    expect(sanitiseSession({ treeWidth: 0 }).treeWidth).toBe(DEFAULT_TREE_WIDTH);
    expect(sanitiseSession({ treeWidth: -5 }).treeWidth).toBe(DEFAULT_TREE_WIDTH);
    expect(sanitiseSession({ treeWidth: Number.NaN }).treeWidth).toBe(DEFAULT_TREE_WIDTH);
    expect(sanitiseSession({ treeWidth: "wide" }).treeWidth).toBe(DEFAULT_TREE_WIDTH);
    // A width from a wider screen is kept; the splitter clamps it to the window.
    expect(sanitiseSession({ treeWidth: 4000 }).treeWidth).toBe(4000);
  });

  it("rejects a sort column that is not one", () => {
    expect(sanitiseSession({ sort: { column: "nope" } }).sort).toEqual(DEFAULT_SORT);
    expect(sanitiseSession({ sort: "bpm" }).sort).toEqual(DEFAULT_SORT);
    expect(sanitiseSession({ sort: { column: "bpm" } }).sort).toEqual({
      column: "bpm",
      descending: false,
    });
  });

  it("treats a missing panel flag as closed", () => {
    const session = sanitiseSession({ infoOpen: "yes" });
    expect(session.infoOpen).toBe(false);
    expect(session.subOpen).toBe(false);
  });

  it("keeps a selected node only when it is an id", () => {
    expect(sanitiseSession({ selectedNodeId: "pl-1" }).selectedNodeId).toBe("pl-1");
    expect(sanitiseSession({ selectedNodeId: 42 }).selectedNodeId).toBeNull();
  });
});
