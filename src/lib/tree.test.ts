import { describe, expect, it } from "vitest";

import type { TreeNode } from "@/ipc/types";
import { branchIds, emptySources, hasChildren, nodesForSource, sourceOf, toggle, visibleNodes } from "./tree";

/** `"a"` at depth 0, `"  b"` at depth 1, and so on. */
function tree(...spec: string[]): TreeNode[] {
  return spec.map((s) => {
    const name = s.trimStart();
    return {
      id: name,
      name,
      kind: "playlist" as const,
      depth: (s.length - name.length) / 2,
    };
  });
}

const NESTED = tree(
  "all",
  "gigs",
  "  friday",
  "    warmup",
  "    peak",
  "  saturday",
  "archive",
  "  2025",
);

describe("visibleNodes", () => {
  it("returns everything when nothing is collapsed", () => {
    expect(visibleNodes(NESTED, new Set()).map((n) => n.id)).toEqual(
      NESTED.map((n) => n.id),
    );
  });

  it("hides everything under a collapsed node", () => {
    const visible = visibleNodes(NESTED, new Set(["gigs"])).map((n) => n.id);
    expect(visible).toEqual(["all", "gigs", "archive", "2025"]);
  });

  it("hides a subtree without hiding its siblings", () => {
    const visible = visibleNodes(NESTED, new Set(["friday"])).map((n) => n.id);
    expect(visible).toEqual(["all", "gigs", "friday", "saturday", "archive", "2025"]);
  });

  it("keeps the collapsed node itself visible", () => {
    expect(visibleNodes(NESTED, new Set(["gigs"])).map((n) => n.id)).toContain("gigs");
  });

  it("handles several collapsed nodes at once, nested and not", () => {
    const visible = visibleNodes(NESTED, new Set(["friday", "archive"])).map((n) => n.id);
    expect(visible).toEqual(["all", "gigs", "friday", "saturday", "archive"]);
  });

  it("a collapsed node inside a collapsed node changes nothing", () => {
    const outer = visibleNodes(NESTED, new Set(["gigs"])).map((n) => n.id);
    const both = visibleNodes(NESTED, new Set(["gigs", "friday"])).map((n) => n.id);
    expect(both).toEqual(outer);
  });

  it("collapsing a leaf does nothing", () => {
    expect(visibleNodes(NESTED, new Set(["peak"])).map((n) => n.id)).toEqual(
      NESTED.map((n) => n.id),
    );
  });

  it("survives an id that is not in the tree", () => {
    expect(visibleNodes(NESTED, new Set(["nope"]))).toHaveLength(NESTED.length);
  });

  it("copes with an empty tree", () => {
    expect(visibleNodes([], new Set(["a"]))).toEqual([]);
  });

  it("does not lose a node whose depth jumps back by more than one", () => {
    // "peak" is depth 2 and "archive" is depth 0: the hidden-depth marker has
    // to clear on any shallower node, not only the next one down.
    const visible = visibleNodes(NESTED, new Set(["friday"])).map((n) => n.id);
    expect(visible).toContain("archive");
  });
});

describe("hasChildren", () => {
  it("is true only when the next node is deeper", () => {
    expect(hasChildren(NESTED, 1)).toBe(true); // gigs
    expect(hasChildren(NESTED, 2)).toBe(true); // friday
    expect(hasChildren(NESTED, 3)).toBe(false); // warmup
    expect(hasChildren(NESTED, 0)).toBe(false); // all, next is a sibling
  });

  it("is false for the last node, which can have nothing under it", () => {
    expect(hasChildren(NESTED, NESTED.length - 1)).toBe(false);
    expect(hasChildren(NESTED, 99)).toBe(false);
  });
});

describe("toggle", () => {
  it("adds then removes", () => {
    const once = toggle(new Set<string>(), "a");
    expect([...once]).toEqual(["a"]);
    expect([...toggle(once, "a")]).toEqual([]);
  });

  it("returns a new set, so React sees the change", () => {
    const before = new Set(["a"]);
    expect(toggle(before, "b")).not.toBe(before);
    expect([...before]).toEqual(["a"]);
  });
});

describe("branchIds", () => {
  it("names every node with something under it, and nothing else", () => {
    // archive counts: 2025 sits under it.
    expect([...branchIds(NESTED)].sort()).toEqual(["archive", "friday", "gigs"]);
  });

  it("agrees with hasChildren on every node", () => {
    // The fast path and the obvious one must not drift apart.
    const ids = branchIds(NESTED);
    NESTED.forEach((node, i) => {
      expect(ids.has(node.id)).toBe(hasChildren(NESTED, i));
    });
  });

  it("is empty for a flat tree", () => {
    expect(branchIds(tree("a", "b", "c")).size).toBe(0);
  });
});

describe("nodesForSource", () => {
  const mixed: TreeNode[] = [
    { id: "all", name: "All Tracks", kind: "allTracks", depth: 0 },
    { id: "pl", name: "Playlists", kind: "collection", depth: 0 },
    { id: "f", name: "Gigs", kind: "folder", depth: 1 },
    { id: "p", name: "Friday", kind: "playlist", depth: 2 },
    { id: "h", name: "2026-09-07", kind: "history", depth: 1 },
  ];

  it("gives playlists the folders and lists", () => {
    expect(nodesForSource(mixed, "playlists").map((n) => n.id)).toEqual(["pl", "f", "p"]);
  });

  it("gives histories the history nodes", () => {
    expect(nodesForSource(mixed, "histories").map((n) => n.id)).toEqual(["h"]);
  });

  it("gives devices nothing, because device support is not built", () => {
    expect(nodesForSource(mixed, "devices")).toEqual([]);
  });

  it("never invents a node", () => {
    const every = (["playlists", "histories", "devices"] as const)
      .flatMap((s) => nodesForSource(mixed, s));
    for (const node of every) expect(mixed).toContain(node);
  });
});

describe("emptySources", () => {
  it("names the sections with nothing in them", () => {
    const only = [{ id: "all", name: "All Tracks", kind: "allTracks" as const, depth: 0 }];
    const empty = emptySources(only);
    expect(empty.has("playlists")).toBe(true);
    expect(empty.has("histories")).toBe(true);
    expect(empty.has("devices")).toBe(true);
  });

  it("calls everything empty for an empty tree", () => {
    expect(emptySources([]).size).toBe(3);
  });
});

describe("sourceOf", () => {
  const mixed: TreeNode[] = [
    { id: "all", name: "All Tracks", kind: "allTracks", depth: 0 },
    { id: "p", name: "Friday", kind: "playlist", depth: 1 },
    { id: "h", name: "2026-09-07", kind: "history", depth: 1 },
  ];

  it("reports the section the selection is in", () => {
    expect(sourceOf(mixed, "p")).toBe("playlists");
    expect(sourceOf(mixed, "h")).toBe("histories");
  });

  it("falls back to playlists, which is where the tree opens", () => {
    // All Tracks included: it sits at the top of the same tree and the rail
    // has no button of its own for it.
    expect(sourceOf(mixed, "all")).toBe("playlists");
    expect(sourceOf(mixed, null)).toBe("playlists");
    expect(sourceOf(mixed, "gone")).toBe("playlists");
  });
});

describe("devices in the tree", () => {
  const nodes: TreeNode[] = [
    { id: "playlists", name: "Playlists", kind: "collection", depth: 0 },
    { id: "pl-1", name: "Warm Up", kind: "playlist", depth: 1 },
    { id: "device:/Volumes/DJ STICK", name: "DJ STICK", kind: "device", depth: 0 },
  ];

  it("shows connected volumes under Devices and nowhere else", () => {
    expect(nodesForSource(nodes, "devices").map((n) => n.name)).toEqual(["DJ STICK"]);
    expect(nodesForSource(nodes, "playlists").map((n) => n.name)).toEqual(["Playlists", "Warm Up"]);
  });

  it("stops dimming Devices once something is connected", () => {
    expect(emptySources(nodes).has("devices")).toBe(false);
    expect(emptySources(nodes.slice(0, 2)).has("devices")).toBe(true);
  });

  it("puts the rail on Devices when a volume is selected", () => {
    expect(sourceOf(nodes, "device:/Volumes/DJ STICK")).toBe("devices");
  });
});
