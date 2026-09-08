import { describe, expect, it } from "vitest";

import type { TreeNode } from "@/ipc/types";
import { branchIds, hasChildren, toggle, visibleNodes } from "./tree";

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
