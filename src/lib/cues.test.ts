import { describe, expect, it } from "vitest";

import type { Cue } from "@/ipc/types";
import { MEMORY_TOLERANCE_MS, memoryCueAt, nextMemoryCue, previousMemoryCue } from "./cues";

const memory = (id: string, positionMs: number): Cue => ({
  id, positionMs, outMs: 0, letter: "", memory: true,
});
const hot = (id: string, letter: string, positionMs: number): Cue => ({
  id, positionMs, outMs: 0, letter, memory: false,
});

// Out of order on purpose: the list the backend hands over is sorted, but
// nothing here should depend on it.
const cues: Cue[] = [
  memory("m3", 90_000),
  hot("a", "A", 5_000),
  memory("m1", 1_000),
  memory("m2", 30_000),
  hot("b", "B", 60_000),
];

describe("nextMemoryCue", () => {
  it("is the first memory cue after the playhead, skipping hot cues", () => {
    expect(nextMemoryCue(cues, 0)?.id).toBe("m1");
    expect(nextMemoryCue(cues, 4_000)?.id).toBe("m2");
    expect(nextMemoryCue(cues, 59_000)?.id).toBe("m3");
  });

  it("moves on from the cue the playhead is standing on", () => {
    expect(nextMemoryCue(cues, 30_000)?.id).toBe("m3");
    expect(nextMemoryCue(cues, 30_000 + MEMORY_TOLERANCE_MS)?.id).toBe("m3");
    expect(nextMemoryCue(cues, 30_000 - MEMORY_TOLERANCE_MS - 1)?.id).toBe("m2");
  });

  it("is null past the last one, and with no memory cues at all", () => {
    expect(nextMemoryCue(cues, 90_000)).toBeNull();
    expect(nextMemoryCue([hot("a", "A", 1)], 0)).toBeNull();
    expect(nextMemoryCue([], 0)).toBeNull();
  });
});

describe("previousMemoryCue", () => {
  it("is the last memory cue before the playhead", () => {
    expect(previousMemoryCue(cues, 100_000)?.id).toBe("m3");
    expect(previousMemoryCue(cues, 60_000)?.id).toBe("m2");
    expect(previousMemoryCue(cues, 2_000)?.id).toBe("m1");
  });

  it("moves back from the cue the playhead is standing on", () => {
    expect(previousMemoryCue(cues, 30_000)?.id).toBe("m1");
    expect(previousMemoryCue(cues, 30_000 - MEMORY_TOLERANCE_MS)?.id).toBe("m1");
    expect(previousMemoryCue(cues, 30_000 + MEMORY_TOLERANCE_MS + 1)?.id).toBe("m2");
  });

  it("is null before the first one", () => {
    expect(previousMemoryCue(cues, 1_000)).toBeNull();
    expect(previousMemoryCue(cues, 0)).toBeNull();
    expect(previousMemoryCue([], 50_000)).toBeNull();
  });
});

describe("memoryCueAt", () => {
  it("is the memory cue within the tolerance, and nothing further", () => {
    expect(memoryCueAt(cues, 30_000)?.id).toBe("m2");
    expect(memoryCueAt(cues, 30_000 + MEMORY_TOLERANCE_MS)?.id).toBe("m2");
    expect(memoryCueAt(cues, 30_000 + MEMORY_TOLERANCE_MS + 1)).toBeNull();
    expect(memoryCueAt(cues, 45_000)).toBeNull();
  });

  it("never picks a hot cue", () => {
    expect(memoryCueAt(cues, 5_000)).toBeNull();
    expect(memoryCueAt(cues, 60_000)).toBeNull();
  });

  it("takes the nearer of two within reach", () => {
    const close = [memory("x", 1_000), memory("y", 1_010)];
    expect(memoryCueAt(close, 1_008)?.id).toBe("y");
    expect(memoryCueAt(close, 1_003)?.id).toBe("x");
  });
});
