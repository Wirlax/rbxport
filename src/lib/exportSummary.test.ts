import { describe, expect, it } from "vitest";

import { exportSummary } from "./exportSummary";

const base = { tracks: 0, reused: 0, removed: 0, skipped: [] as string[], verified: true };

describe("exportSummary", () => {
  it("reports a first export by its size", () => {
    expect(exportSummary("Set", { ...base, tracks: 12 })).toBe(
      "Exported 12 tracks to Set. read back and verified.",
    );
  });

  it("reports a second export by what changed", () => {
    expect(exportSummary("Set", { ...base, tracks: 12, reused: 10, removed: 3 })).toBe(
      "Synced Set: 2 tracks copied, 10 unchanged, 3 tracks removed. read back and verified.",
    );
  });

  it("leaves out the parts that are zero", () => {
    expect(exportSummary("Set", { ...base, tracks: 10, reused: 10 })).toBe(
      "Synced Set: 10 unchanged. read back and verified.",
    );
  });

  it("counts one track as a track", () => {
    expect(exportSummary("Set", { ...base, tracks: 1 })).toContain("1 track to Set");
    expect(exportSummary("Set", { ...base, tracks: 2, reused: 1, removed: 1 })).toContain(
      "1 track copied",
    );
  });

  it("says when tracks were left behind", () => {
    expect(exportSummary("Set", { ...base, tracks: 3, skipped: ["Gone"] })).toContain(
      "1 track skipped — the audio was missing",
    );
  });

  it("says plainly when the stick did not read back", () => {
    expect(exportSummary("Set", { ...base, tracks: 3, verified: false })).toContain(
      "did not read back",
    );
  });
});
