import { describe, expect, it } from "vitest";

import { nextPeak } from "./useMaster";

describe("nextPeak", () => {
  it("takes a louder reading at once", () => {
    // Fast attack: the transient is the thing worth seeing.
    expect(nextPeak(0.2, 0.9)).toBe(0.9);
  });

  it("falls back rather than dropping when the reading is quieter", () => {
    // Slow release: a meter that took every reading would flicker.
    expect(nextPeak(1, 0)).toBeCloseTo(0.8, 6);
    expect(nextPeak(0.8, 0)).toBeCloseTo(0.64, 6);
  });

  it("reaches silence rather than hovering above it forever", () => {
    let shown = 1;
    for (let i = 0; i < 60; i++) shown = nextPeak(shown, 0);
    expect(shown).toBeLessThan(0.001);
  });

  it("never leaves the bar, whatever it is handed", () => {
    for (const [shown, reading] of [
      [Number.NaN, 0.5], [0.5, Number.NaN], [-1, -1], [2, 3],
    ] as const) {
      const next = nextPeak(shown, reading);
      expect(next).toBeGreaterThanOrEqual(0);
      expect(next).toBeLessThanOrEqual(1);
    }
  });
});
