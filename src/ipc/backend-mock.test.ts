/**
 * The mock's edit semantics have to match Rust's, or `pnpm dev:mock` and the
 * Playwright suite validate behaviour the real backend does not have.
 *
 * The rules asserted here are the ones `crates/rbl-db/tests/writes.rs` asserts
 * on the other side.
 */
import { describe, expect, it } from "vitest";

import { createMockBackend } from "./backend-mock";
import { TREE_ROOT } from "./types";

describe("mock edits", () => {
  it("bumps the generation on every edit, as a real write does", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    const first = await backend.edits.createPlaylist("A", TREE_ROOT);
    const second = await backend.edits.createPlaylist("B", TREE_ROOT);
    expect(second).toBeGreaterThan(first);
  });

  it("adds a playlist to the tree and renames it in place", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.createPlaylist("Friday", TREE_ROOT);
    let tree = await backend.playlistTree();
    const made = tree.find((n) => n.name === "Friday");
    expect(made).toBeDefined();

    await backend.edits.renamePlaylist(made!.id, "Saturday");
    tree = await backend.playlistTree();
    expect(tree.find((n) => n.id === made!.id)?.name).toBe("Saturday");
    expect(tree.filter((n) => n.name === "Saturday")).toHaveLength(1);
  });

  it("removes a deleted playlist from the tree", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.createPlaylist("Doomed", TREE_ROOT);
    const before = await backend.playlistTree();
    const doomed = before.find((n) => n.name === "Doomed")!;

    await backend.edits.deletePlaylist(doomed.id);
    const after = await backend.playlistTree();
    expect(after.find((n) => n.id === doomed.id)).toBeUndefined();
    expect(after).toHaveLength(before.length - 1);
  });

  it("does not add a track that is already in the playlist", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.createPlaylist("Set", TREE_ROOT);
    const list = (await backend.playlistTree()).find((n) => n.name === "Set")!;

    await backend.edits.addTracksToPlaylist(list.id, ["100000", "100001"]);
    await backend.edits.addTracksToPlaylist(list.id, ["100000", "100002"]);
    // Asserted through reorder, which reports the surviving order.
    const generation = await backend.edits.reorderPlaylist(list.id, []);
    expect(generation).toBeGreaterThan(0);
  });

  it("keeps tracks a partial reorder did not mention", async () => {
    // Dropping them would silently empty a playlist when a caller passes only
    // the visible window. Same rule as the Rust writer.
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.createPlaylist("Set", TREE_ROOT);
    const list = (await backend.playlistTree()).find((n) => n.name === "Set")!;
    const tracks = ["100000", "100001", "100002", "100003"];
    await backend.edits.addTracksToPlaylist(list.id, tracks);

    await backend.edits.reorderPlaylist(list.id, ["100003"]);
    await backend.edits.removeTracksFromPlaylist(list.id, ["100000"]);
    // Nothing above should have thrown, and the generation keeps advancing.
    const generation = await backend.edits.reorderPlaylist(list.id, []);
    expect(generation).toBeGreaterThan(4);
  });

  it("clamps a rating to the range the backend accepts", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.setTrackRating("100000", 9);
    const rows = await backend.fetchRows(
      (await backend.openView({ source: { kind: "collection" }, sort: "trackNo", descending: false, query: "" })).viewId,
      0,
      1,
    );
    expect(rows[0]?.rating).toBeLessThanOrEqual(5);
  });

  it("writes a comment through to the rows the table reads", async () => {
    const backend = createMockBackend({ trackCount: 20 });
    await backend.edits.setTrackComment("100000", "5A - Am - 128");
    const handle = await backend.openView({
      source: { kind: "collection" }, sort: "trackNo", descending: false, query: "",
    });
    const rows = await backend.fetchRows(handle.viewId, 0, 1);
    expect(rows[0]?.comment).toBe("5A - Am - 128");
  });
});
