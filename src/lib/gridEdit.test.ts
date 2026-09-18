import { describe, expect, it } from "vitest";

import {
  applyEdit, applyEditFrom, tapTempo, tempoX100, withTap, TAP_GAP_MS, type EditableBeat,
} from "./gridEdit";

/** Eight beats at 120 BPM from 500 ms, numbered from the first — the same
 * grid `rbl_anlz::grid`'s tests use, so the two stay in step case by case. */
function grid(): EditableBeat[] {
  return Array.from({ length: 8 }, (_, i) => ({ number: (i % 4) + 1, tempoX100: 12_000, timeMs: 500 + i * 500 }));
}
const times = (beats: readonly EditableBeat[]) => beats.map((b) => b.timeMs);
const numbers = (beats: readonly EditableBeat[]) => beats.map((b) => b.number);

describe("applyEdit, as the Rust module does it", () => {
  it("leaves an empty grid empty", () => {
    expect(applyEdit([], { kind: "double" })).toEqual([]);
    expect(applyEditFrom([], 100, { kind: "nudge", ms: 1 })).toEqual([]);
  });

  it("nudges every beat and drops those pushed off the front", () => {
    expect(times(applyEdit(grid(), { kind: "nudge", ms: -40 }))).toEqual([460, 960, 1460, 1960, 2460, 2960, 3460, 3960]);
    const off = applyEdit(grid(), { kind: "nudge", ms: -1200 });
    expect(times(off)).toEqual([300, 800, 1300, 1800, 2300, 2800]);
    expect(numbers(off)).toEqual([3, 4, 1, 2, 3, 4]);
  });

  it("doubles and halves around the downbeat", () => {
    const doubled = applyEdit(grid(), { kind: "double" });
    expect(doubled).toHaveLength(15);
    expect(times(doubled).slice(0, 5)).toEqual([500, 750, 1000, 1250, 1500]);
    expect(numbers(doubled).slice(0, 5)).toEqual([1, 2, 3, 4, 1]);
    expect(tempoX100(doubled)).toBe(24_000);
    const halved = applyEdit(grid(), { kind: "halve" });
    expect(times(halved)).toEqual([500, 1500, 2500, 3500]);
    expect(numbers(halved)).toEqual([1, 2, 3, 4]);
    expect(tempoX100(halved)).toBe(6_000);
    expect(applyEdit(doubled, { kind: "halve" })).toEqual(grid());
  });

  it("makes the nearest beat the downbeat without moving it", () => {
    const fixed = applyEdit(grid(), { kind: "downbeat", timeMs: 1600 });
    expect(numbers(fixed)).toEqual([3, 4, 1, 2, 3, 4, 1, 2]);
    expect(times(fixed)).toEqual(times(grid()));
  });

  it("lays the grid out at a new tempo around the anchor", () => {
    const slowed = applyEdit(grid(), { kind: "tempo", bpmX100: 10_000, anchorMs: 2000 });
    expect(times(slowed)).toEqual([800, 1400, 2000, 2600, 3200, 3800]);
    expect(numbers(slowed)).toEqual([2, 3, 4, 1, 2, 3]);
    const tapped = applyEdit(grid(), { kind: "tempo", bpmX100: 12_000, anchorMs: 2100 });
    expect(times(tapped)).toEqual([600, 1100, 1600, 2100, 2600, 3100, 3600, 4100]);
    expect(applyEdit(grid(), { kind: "tempo", bpmX100: 0, anchorMs: 500 })).toEqual(grid());
    expect(times(applyEdit(grid(), { kind: "tempo", bpmX100: 12_000, anchorMs: 200 })).slice(0, 2)).toEqual([200, 700]);
  });

  it("stretches by hundredths with the first beat held", () => {
    const narrower = applyEdit(grid(), { kind: "stretch", byX100: 1 });
    expect(tempoX100(narrower)).toBe(12_001);
    expect(times(narrower)).toEqual(times(grid()));
    const wider = applyEdit(grid(), { kind: "stretch", byX100: -1000 });
    expect(times(wider).slice(0, 3)).toEqual([500, 1045, 1591]);
    expect(tempoX100(applyEdit(grid(), { kind: "stretch", byX100: -20_000 }))).toBe(1);
  });

  it("aligns the nearest beat to the time", () => {
    expect(times(applyEdit(grid(), { kind: "align", timeMs: 1530 }))).toEqual([530, 1030, 1530, 2030, 2530, 3030, 3530, 4030]);
    expect(times(applyEdit(grid(), { kind: "align", timeMs: 1470 })).slice(0, 3)).toEqual([470, 970, 1470]);
  });

  it("applies from a point, keeping the head and dropping tail beats that would cross it", () => {
    const split = applyEditFrom(grid(), 2480, { kind: "tempo", bpmX100: 10_000, anchorMs: 2500 });
    expect(times(split)).toEqual([500, 1000, 1500, 2000, 2500, 3100, 3700]);
    expect(numbers(split)).toEqual([1, 2, 3, 4, 1, 2, 3]);
    expect(split[0]?.tempoX100).toBe(12_000);
    expect(split[4]?.tempoX100).toBe(10_000);
    expect(times(applyEditFrom(grid(), 2500, { kind: "nudge", ms: -700 }))).toEqual([500, 1000, 1500, 2000, 2300, 2800, 3300]);
    expect(times(applyEditFrom(grid(), 2500, { kind: "halve" }))).toEqual([500, 1000, 1500, 2000, 2500, 3500]);
    expect(applyEditFrom(grid(), null, { kind: "nudge", ms: 10 })).toEqual(applyEdit(grid(), { kind: "nudge", ms: 10 }));
  });
});

describe("tapTempo", () => {
  it("needs two taps and averages the last eight", () => {
    expect(tapTempo([])).toBeNull();
    expect(tapTempo([1000])).toBeNull();
    expect(tapTempo([1000, 1500])).toBe(12_000);
    // 128 BPM is 468.75 ms a beat; nine taps keep the last eight.
    const taps = Array.from({ length: 9 }, (_, i) => 100 + i * 468.75);
    expect(tapTempo(taps)).toBe(12_800);
    // A stray early tap washes out once enough follow it.
    expect(tapTempo([0, 900, 1400, 1900, 2400, 2900, 3400, 3900, 4400])).toBe(12_000);
  });

  it("starts a new run after a gap, or when time runs backwards", () => {
    expect(withTap([], 100)).toEqual([100]);
    expect(withTap([100], 600)).toEqual([100, 600]);
    expect(withTap([100, 600], 600 + TAP_GAP_MS + 1)).toEqual([600 + TAP_GAP_MS + 1]);
    expect(withTap([100, 600], 50)).toEqual([50]);
  });
});
