import { describe, expect, it } from "vitest";

import {
  BEATS_PER_BAR,
  DETAIL_BARS,
  detailSpan,
  headPercent,
  phraseKind,
  phraseSpans,
  splitTime,
  windowAround,
} from "./player";

describe("detailSpan", () => {
  it("shows the same number of bars whatever the tempo", () => {
    // Twelve bars at 128 BPM is 22.5 s. A fixed fraction cannot do this: on a
    // three-minute track and a ninety-minute mix it shows wildly different
    // amounts of music, and neither matches a CDJ.
    const seconds = (DETAIL_BARS * BEATS_PER_BAR * 60) / 128;
    expect(seconds).toBeCloseTo(22.5, 5);
    expect(detailSpan(DETAIL_BARS, 12_800, 338)).toBeCloseTo(seconds / 338, 6);
    // Same bars, faster track: a smaller slice of the same length.
    expect(detailSpan(DETAIL_BARS, 17_400, 338)).toBeLessThan(
      detailSpan(DETAIL_BARS, 12_800, 338),
    );
  });

  it("covers twice as much at twice the bars", () => {
    expect(detailSpan(24, 12_800, 338)).toBeCloseTo(detailSpan(12, 12_800, 338) * 2, 6);
  });

  it("never exceeds the whole track", () => {
    // Twelve bars of a very slow, very short track is longer than the track.
    expect(detailSpan(DETAIL_BARS, 6_000, 5)).toBe(1);
  });

  it("falls back rather than dividing by zero", () => {
    for (const [bpm, dur] of [[0, 338], [12_800, 0], [0, 0]] as const) {
      const span = detailSpan(DETAIL_BARS, bpm, dur);
      expect(span).toBeGreaterThan(0);
      expect(span).toBeLessThanOrEqual(1);
    }
  });
});

describe("windowAround", () => {
  it("centres on the playhead in the middle of a track", () => {
    expect(windowAround(0.5, 0.1)).toEqual({ from: 0.45, to: 0.55 });
  });

  it("pins at either end rather than running off the track", () => {
    expect(windowAround(0, 0.1)).toEqual({ from: 0, to: 0.1 });
    expect(windowAround(1, 0.1).to).toBeCloseTo(1, 6);
    expect(windowAround(1, 0.1).from).toBeCloseTo(0.9, 6);
  });

  it("handles a span wider than the track", () => {
    expect(windowAround(0.3, 5)).toEqual({ from: 0, to: 1 });
  });
});

describe("headPercent", () => {
  it("sits at the centre for most of a track", () => {
    expect(headPercent(0.5, 0.1)).toBe(50);
  });

  it("crosses the window at the ends, so the start is reachable", () => {
    expect(headPercent(0, 0.1)).toBe(0);
    expect(headPercent(1, 0.1)).toBeCloseTo(100, 5);
    expect(headPercent(0.025, 0.1)).toBeCloseTo(25, 5);
  });
});

describe("splitTime", () => {
  it("splits minutes from tenths, the way rekordbox prints them", () => {
    expect(splitTime(338.1)).toEqual({ main: "5:38", tenths: "1" });
    expect(splitTime(0)).toEqual({ main: "0:00", tenths: "0" });
    expect(splitTime(65.95)).toEqual({ main: "1:05", tenths: "9" });
  });

  it("never prints a negative or a NaN", () => {
    expect(splitTime(-5).main).toBe("0:00");
    expect(splitTime(Number.NaN).main).toBe("0:00");
  });
});

describe("phraseKind", () => {
  it("gives each UP its own colour, as rekordbox does", () => {
    expect(phraseKind("UP 1")).toBe("up");
    expect(phraseKind("UP 2")).toBe("up2");
    expect(phraseKind("UP 3")).toBe("up3");
  });

  it("keys everything else off its first word", () => {
    expect(phraseKind("INTRO 2")).toBe("intro");
    expect(phraseKind("CHORUS 1")).toBe("chorus");
    expect(phraseKind("DOWN")).toBe("down");
    expect(phraseKind("OUTRO")).toBe("outro");
    expect(phraseKind("OUT")).toBe("outro");
  });

  it("still colours a label it does not know", () => {
    // A hole in the phrase bar reads as missing analysis, which is a
    // different problem from an unfamiliar label.
    expect(phraseKind("SOMETHING ELSE")).toBe("verse");
  });
});

describe("phraseSpans", () => {
  const phrases = [
    { timeMs: 0, beat: 1, label: "INTRO 2" },
    { timeMs: 30_000, beat: 65, label: "CHORUS 1" },
    { timeMs: 60_000, beat: 129, label: "DOWN" },
  ];

  it("runs each phrase to the start of the next, and the last to the end", () => {
    const spans = phraseSpans(phrases, 90_000);
    expect(spans.map((s) => [s.from, s.to])).toEqual([
      [0, 1 / 3],
      [1 / 3, 2 / 3],
      [2 / 3, 1],
    ]);
    expect(spans.map((s) => s.kind)).toEqual(["intro", "chorus", "down"]);
  });

  it("sorts before laying out, so an out-of-order tag still reads left to right", () => {
    const spans = phraseSpans([...phrases].reverse(), 90_000);
    expect(spans.map((s) => s.label)).toEqual(["INTRO 2", "CHORUS 1", "DOWN"]);
  });

  it("drops a zero-width phrase rather than drawing an invisible block", () => {
    const spans = phraseSpans(
      [{ timeMs: 0, beat: 1, label: "INTRO" }, { timeMs: 0, beat: 1, label: "CHORUS" }],
      90_000,
    );
    expect(spans).toHaveLength(1);
  });

  it("returns nothing for a track with no length", () => {
    expect(phraseSpans(phrases, 0)).toEqual([]);
  });

  it("clamps a phrase that starts past the end", () => {
    const spans = phraseSpans([{ timeMs: 999_999, beat: 9_999, label: "OUTRO" }], 90_000);
    expect(spans).toEqual([]);
  });

  it("places a phrase the beat grid did not reach, when the tempo is known", () => {
    // `timeMs` is absent on a track whose analysis is older than its length.
    // A beat and a tempo still say where the phrase is.
    const beatMs = 60_000 / 128;
    const spans = phraseSpans(
      [
        { timeMs: null, beat: 1, label: "INTRO" },
        { timeMs: null, beat: 129, label: "CHORUS" },
      ],
      90_000,
      beatMs,
    );
    expect(spans).toHaveLength(2);
    expect(spans[1]?.from).toBeCloseTo((128 * beatMs) / 90_000, 6);
  });

  it("drops an unresolved phrase rather than stacking it on the left edge", () => {
    // With no tempo there is nothing to place it from, and a block at zero
    // would read as a mangled track rather than as missing data.
    const spans = phraseSpans(
      [
        { timeMs: null, beat: 1, label: "INTRO" },
        { timeMs: 30_000, beat: 65, label: "CHORUS" },
      ],
      90_000,
    );
    expect(spans.map((s) => s.label)).toEqual(["CHORUS"]);
  });
});
