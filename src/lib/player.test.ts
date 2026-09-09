import { describe, expect, it } from "vitest";

import {
  BEATS_PER_BAR,
  DETAIL_BARS,
  beatsIn,
  detailSpan,
  dragSeconds,
  headPercent,
  phraseKind,
  phraseSpans,
  splitTime,
  memoryTime,
  cuesFor,
  nearestBeatMs,
  NO_BEATS,
  pressCue,
  releaseCue,
  parseBeatGrid,
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
  it("centres on the playhead", () => {
    expect(windowAround(0.5, 0.1)).toEqual({ from: 0.45, to: 0.55 });
  });

  it("runs off the ends rather than sliding the head across the strip", () => {
    // The head is the middle of the strip, always. At the start of a track the
    // window hangs off the front and the left half draws empty — pinning it
    // instead makes the head mean "somewhere in here" for the first bars.
    expect(windowAround(0, 0.1)).toEqual({ from: -0.05, to: 0.05 });
    expect(windowAround(1, 0.1).from).toBeCloseTo(0.95, 6);
    expect(windowAround(1, 0.1).to).toBeCloseTo(1.05, 6);
  });

  it("handles a span wider than the track", () => {
    expect(windowAround(0.3, 5)).toEqual({ from: -2.2, to: 2.8 });
  });
});

describe("headPercent", () => {
  it("is the middle of the strip wherever the track is", () => {
    expect(headPercent()).toBe(50);
  });
});

describe("dragSeconds", () => {
  it("pulls the track the way the pointer moves", () => {
    // Dragging right brings earlier music into view, so the playhead goes back.
    expect(dragSeconds(100, 1000, 0.1, 300)).toBeCloseTo(-3, 6);
    expect(dragSeconds(-100, 1000, 0.1, 300)).toBeCloseTo(3, 6);
  });

  it("moves less music per pixel the further it is zoomed in", () => {
    expect(Math.abs(dragSeconds(50, 1000, 0.02, 300)))
      .toBeLessThan(Math.abs(dragSeconds(50, 1000, 0.2, 300)));
  });

  it("returns nothing rather than a NaN when there is nothing to drag", () => {
    for (const [dx, w, span, dur] of [
      [10, 0, 0.1, 300], [10, 100, 0.1, 0], [Number.NaN, 100, 0.1, 300],
    ] as const) {
      expect(dragSeconds(dx, w, span, dur)).toBe(0);
    }
  });
});

describe("splitTime", () => {
  it("splits minutes from tenths, the way rekordbox prints them", () => {
    expect(splitTime(338.1)).toEqual({ main: "05:38", tenths: "1" });
    expect(splitTime(0)).toEqual({ main: "00:00", tenths: "0" });
    expect(splitTime(65.95)).toEqual({ main: "01:05", tenths: "9" });
  });

  it("pads the minutes, so the readout does not shift at ten minutes", () => {
    // Unpadded, the whole row of readouts beside it moves by a character.
    expect(splitTime(599).main).toHaveLength(5);
    expect(splitTime(601).main).toHaveLength(5);
  });

  it("never prints a negative or a NaN", () => {
    expect(splitTime(-5).main).toBe("00:00");
    expect(splitTime(Number.NaN).main).toBe("00:00");
  });
});

describe("memoryTime", () => {
  it("prints to the millisecond, minutes padded like everything else", () => {
    expect(memoryTime(46)).toBe("00:00:046");
    expect(memoryTime(165_046)).toBe("02:45:046");
  });

  it("agrees with splitTime about the minutes and seconds", () => {
    // Two lists disagreeing about padding is what stops columns lining up.
    expect(memoryTime(338_100).startsWith(splitTime(338.1).main)).toBe(true);
  });

  it("never prints a negative or a NaN", () => {
    expect(memoryTime(-1)).toBe("00:00:000");
    expect(memoryTime(Number.NaN)).toBe("00:00:000");
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

describe("cuesFor", () => {
  const cues = [
    { memory: false, letter: "B", positionMs: 30_000 },
    { memory: true, letter: "", positionMs: 60_000 },
    { memory: false, letter: "A", positionMs: 10_000 },
    { memory: true, letter: "", positionMs: 5_000 },
  ];

  it("splits the one list the two tabs share", () => {
    // Memory cues and hot cues are the same rows told apart by a flag.
    expect(cuesFor(cues, "memory").map((c) => c.positionMs)).toEqual([5_000, 60_000]);
    expect(cuesFor(cues, "hotCue").map((c) => c.letter)).toEqual(["A", "B"]);
  });

  it("orders by position, so the list can be read as the track", () => {
    expect(cuesFor(cues, "hotCue").map((c) => c.positionMs)).toEqual([10_000, 30_000]);
  });

  it("lists nothing for the info tab, which shows fields instead", () => {
    expect(cuesFor(cues, "info")).toEqual([]);
  });

  it("does not reorder the array it was given", () => {
    const before = [...cues];
    cuesFor(cues, "memory");
    expect(cues).toEqual(before);
  });
});

describe("parseBeatGrid", () => {
  /** The backend's encoding: a little-endian `u32` of ms, then the beat's number. */
  const encode = (beats: readonly [number, number][]): Uint8Array => {
    const bytes = new Uint8Array(beats.length * 5);
    const view = new DataView(bytes.buffer);
    beats.forEach(([ms, number], i) => {
      view.setUint32(i * 5, ms, true);
      view.setUint8(i * 5 + 4, number);
    });
    return bytes;
  };

  it("reads the five-byte records the backend writes", () => {
    const grid = parseBeatGrid(encode([[0, 1], [469, 2], [938, 3], [1407, 4]]));
    expect(Array.from(grid.times)).toEqual([0, 469, 938, 1407]);
    expect(Array.from(grid.numbers)).toEqual([1, 2, 3, 4]);
  });

  it("holds a whole track without turning it into objects", () => {
    // A three-hour mix is about 23,000 beats — a list of objects here is what
    // the windowed fetch existed to avoid.
    const beats: [number, number][] = Array.from({ length: 23_000 }, (_, i) => [i * 469, (i % 4) + 1]);
    const grid = parseBeatGrid(encode(beats));
    expect(grid.times.length).toBe(23_000);
    expect(grid.times).toBeInstanceOf(Uint32Array);
  });

  it("drops a truncated last record rather than reading past it", () => {
    const grid = parseBeatGrid(encode([[0, 1], [469, 2]]).subarray(0, 7));
    expect(Array.from(grid.times)).toEqual([0]);
  });

  it("reads a view into a larger buffer, which is what the IPC hands back", () => {
    const whole = new Uint8Array(15);
    whole.set(encode([[1_000, 1]]), 5);
    const grid = parseBeatGrid(whole.subarray(5, 10));
    expect(Array.from(grid.times)).toEqual([1_000]);
  });

  it("gives an empty grid for a track with no analysis", () => {
    expect(parseBeatGrid(new Uint8Array()).times.length).toBe(0);
  });
});

describe("beatsIn", () => {
  const grid = parseBeatGrid(
    (() => {
      const bytes = new Uint8Array(1_000 * 5);
      const view = new DataView(bytes.buffer);
      for (let i = 0; i < 1_000; i++) {
        view.setUint32(i * 5, i * 500, true);
        view.setUint8(i * 5 + 4, (i % 4) + 1);
      }
      return bytes;
    })(),
  );

  it("returns the beats inside the window, ends included", () => {
    const found = beatsIn(grid, 1_000, 2_000);
    expect(found.map((b) => b.timeMs)).toEqual([1_000, 1_500, 2_000]);
  });

  it("marks the first beat of a bar as the downbeat", () => {
    expect(beatsIn(grid, 0, 1_500).map((b) => b.downbeat)).toEqual([true, false, false, false]);
  });

  it("reads only the window, however long the track", () => {
    // The point of the binary search: a window near the end of a long mix must
    // not cost a scan of everything before it.
    expect(beatsIn(grid, 499_000, 499_500)).toHaveLength(2);
  });

  it("is empty for a window past the end, and for one the wrong way round", () => {
    expect(beatsIn(grid, 1_000_000, 1_100_000)).toEqual([]);
    expect(beatsIn(grid, 2_000, 1_000)).toEqual([]);
  });
});

describe("pressCue", () => {
  it("stops and rewinds to the cue point while playing", () => {
    expect(pressCue(90, 30, true)).toEqual({ seekTo: 30, playing: false, cuePoint: 30 });
  });

  it("previews from the cue point when paused on it", () => {
    // Held, not toggled: the deck plays while the button is down.
    expect(pressCue(30, 30, false)).toEqual({ seekTo: null, playing: true, cuePoint: 30 });
    expect(pressCue(30.01, 30, false).playing).toBe(true);
  });

  it("sets the cue point when paused anywhere else", () => {
    // A CDJ has no separate "set cue" button because this is it.
    expect(pressCue(75.5, 30, false)).toEqual({ seekTo: null, playing: false, cuePoint: 75.5 });
  });

  it("never sets a cue point before the start of the track", () => {
    expect(pressCue(-4, 30, false).cuePoint).toBe(0);
  });
});

describe("releaseCue", () => {
  it("snaps back and stops after a held preview", () => {
    expect(releaseCue(true, 30)).toEqual({ seekTo: 30, playing: false, cuePoint: 30 });
  });

  it("does nothing after any other press", () => {
    // Rewinding here would undo the jump the press itself just made.
    expect(releaseCue(false, 30)).toBeNull();
  });
});

describe("nearestBeatMs", () => {
  const grid = { times: new Uint32Array([0, 500, 1000, 1500]), numbers: new Uint8Array([1, 2, 3, 4]) };

  it("snaps to whichever beat is closer", () => {
    expect(nearestBeatMs(grid, 460)).toBe(500);
    expect(nearestBeatMs(grid, 240)).toBe(0);
    expect(nearestBeatMs(grid, 260)).toBe(500);
  });

  it("takes the earlier beat on a tie, so a snap is repeatable", () => {
    expect(nearestBeatMs(grid, 250)).toBe(0);
  });

  it("clamps to the ends rather than running off the grid", () => {
    expect(nearestBeatMs(grid, -900)).toBe(0);
    expect(nearestBeatMs(grid, 99_000)).toBe(1500);
  });

  it("leaves the position alone when the track has no grid", () => {
    expect(nearestBeatMs(NO_BEATS, 1234)).toBe(1234);
  });
});

describe("pressCue with quantize on", () => {
  const grid = { times: new Uint32Array([0, 500, 1000, 1500]), numbers: new Uint8Array([1, 2, 3, 4]) };

  it("puts a new cue point on the nearest beat, and the playhead with it", () => {
    // The head has to move too: a cue point it is not standing on reads as
    // "paused somewhere else", and the next press would move the cue again.
    expect(pressCue(0.46, 9, false, grid)).toEqual({ seekTo: 0.5, playing: false, cuePoint: 0.5 });
  });

  it("leaves the cue where the finger put it when Q is off", () => {
    expect(pressCue(0.46, 9, false).cuePoint).toBeCloseTo(0.46, 6);
  });

  it("still previews and still rewinds, quantized or not", () => {
    expect(pressCue(0.5, 0.5, false, grid).playing).toBe(true);
    expect(pressCue(90, 30, true, grid)).toEqual({ seekTo: 30, playing: false, cuePoint: 30 });
  });
});
