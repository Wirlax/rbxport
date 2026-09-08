import { describe, expect, it } from "vitest";

import { DEFAULT_SORT, nextSort, specForNode } from "./viewSpec";
import type { TreeNode } from "@/ipc/types";

const playlist: TreeNode = { id: "pl-1", name: "Warm Up", kind: "playlist", depth: 1 };
const folder: TreeNode = { id: "f-1", name: "SHOWS", kind: "folder", depth: 1 };
const device: TreeNode = { id: "device:/Volumes/X", name: "X", kind: "device", depth: 0 };

describe("specForNode", () => {
  it("narrows to a playlist", () => {
    expect(specForNode(playlist, "", null).source).toEqual({ kind: "playlist", id: "pl-1" });
  });

  it("shows the collection for anything that is not a playlist", () => {
    // A folder holds playlists rather than tracks, and a device is not a track
    // source at all — both would otherwise ask for a view that cannot exist.
    for (const node of [folder, device, null]) {
      expect(specForNode(node, "", null).source).toEqual({ kind: "collection" });
    }
  });

  it("carries the query and the sort through", () => {
    const spec = specForNode(playlist, "artbat", { column: "bpm", descending: true });
    expect(spec.query).toBe("artbat");
    expect(spec.sort).toBe("bpm");
    expect(spec.descending).toBe(true);
  });

  it("falls back to the default order when nothing is chosen", () => {
    const spec = specForNode(playlist, "", null);
    expect(spec.sort).toBe(DEFAULT_SORT.column);
    expect(spec.descending).toBe(DEFAULT_SORT.descending);
  });
});

describe("nextSort", () => {
  it("cycles ascending, descending, then back to the default", () => {
    const first = nextSort(DEFAULT_SORT, "bpm");
    expect(first).toEqual({ column: "bpm", descending: false });
    const second = nextSort(first, "bpm");
    expect(second).toEqual({ column: "bpm", descending: true });
    expect(nextSort(second, "bpm")).toEqual(DEFAULT_SORT);
  });

  it("starts a different column ascending rather than continuing the cycle", () => {
    expect(nextSort({ column: "bpm", descending: true }, "artist")).toEqual({
      column: "artist",
      descending: false,
    });
  });
});
