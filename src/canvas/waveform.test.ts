import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { bandStops, drawBands, ramp } from "./waveform";

describe("the three-band colour ramp", () => {
  it("runs blue, amber, cream rather than blue to cream", () => {
    // rekordbox colours a column by its frequency content. A two-stop ramp
    // turns every mid-heavy track grey-blue, which is what this replaced.
    const stops = bandStops("detail");
    expect(ramp(stops, 0)).toBe("rgb(0,85,225)");
    expect(ramp(stops, 0.5)).toBe("rgb(178,101,5)");
    expect(ramp(stops, 1)).toBe("rgb(245,234,214)");
  });

  it("gives the overview a brighter amber than the detail", () => {
    // Two tokens, --c-wave-mid and --c-wave-mid-ovw, and they differ.
    expect(ramp(bandStops("overview"), 0.5)).toBe("rgb(255,140,0)");
    expect(ramp(bandStops("overview"), 0.5)).not.toBe(ramp(bandStops("detail"), 0.5));
  });

  it("interpolates between the stops rather than stepping", () => {
    const stops = bandStops("detail");
    const quarter = ramp(stops, 0.25);
    expect(quarter).not.toBe(ramp(stops, 0));
    expect(quarter).not.toBe(ramp(stops, 0.5));
  });

  it("clamps anything outside the range, including nonsense", () => {
    const stops = bandStops("detail");
    expect(ramp(stops, -1)).toBe(ramp(stops, 0));
    expect(ramp(stops, 99)).toBe(ramp(stops, 1));
    expect(ramp(stops, Number.NaN)).toBe(ramp(stops, 0));
  });

  it("matches the tokens the stylesheet ships", () => {
    // The canvas cannot read a CSS variable per column, so the palette is
    // duplicated here. This is the guard against the two drifting apart.
    const css = readFileSync("src/styles/tokens.css", "utf8");
    const token = (name: string) =>
      new RegExp(`--c-wave-${name}:\\s*(#[0-9A-Fa-f]{6})`).exec(css)?.[1]?.toUpperCase();
    const hex = (rgb: string) => {
      const [r, g, b] = rgb.slice(4, -1).split(",").map(Number);
      return `#${[r, g, b].map((n) => (n ?? 0).toString(16).padStart(2, "0")).join("")}`.toUpperCase();
    };
    expect(hex(ramp(bandStops("detail"), 0))).toBe(token("low"));
    expect(hex(ramp(bandStops("detail"), 0.5))).toBe(token("mid"));
    expect(hex(ramp(bandStops("overview"), 0.5))).toBe(token("mid-ovw"));
    expect(hex(ramp(bandStops("detail"), 1))).toBe(token("high"));
  });
});

describe("the three-band waveform", () => {
  /** A tiny 2D context that records what was filled. */
  function recorder() {
    const fills: { style: string; x: number; y: number; w: number; h: number }[] = [];
    const ctx = {
      fillStyle: "",
      clearRect: () => undefined,
      fillRect(x: number, y: number, w: number, h: number) {
        fills.push({ style: String(this.fillStyle), x, y, w, h });
      },
    };
    return { ctx: ctx as unknown as CanvasRenderingContext2D, fills };
  }

  it("draws each band in its own colour, brightest last", () => {
    // One column: low 60, mid 30, high 10, at PWV6's six-bit full scale.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([60, 30, 10]), 1, 100, "overview");
    expect(fills).toHaveLength(3);
    expect(fills.map((f) => f.style)).toEqual([
      ramp(bandStops("overview"), 0),
      ramp(bandStops("overview"), 0.5),
      ramp(bandStops("overview"), 1),
    ]);
    // Low is the outer envelope; the bright core is smallest and on top.
    expect(fills[0]!.h).toBeGreaterThan(fills[1]!.h);
    expect(fills[1]!.h).toBeGreaterThan(fills[2]!.h);
  });

  it("centres every band on the middle line", () => {
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([60, 30, 10]), 1, 100, "overview");
    for (const fill of fills) {
      expect(fill.y + fill.h / 2).toBeCloseTo(50, 5);
    }
  });

  it("puts both tags on the same seven-bit scale", () => {
    // Measured across 80 tracks: PWV7 reaches 127 and PWV6 reaches 98. An
    // earlier reading of 63 came from one track and drew every overview at
    // double height.
    for (const band of ["overview", "detail"] as const) {
      const r = recorder();
      drawBands(r.ctx, new Uint8Array([127, 0, 0]), 1, 100, band);
      expect(r.fills[0]!.h).toBeCloseTo(100, 5);
    }
  });

  it("stacks the bands from the bottom for a row preview", () => {
    // The track list draws a half waveform: blue from the baseline, amber on
    // top of it, near-white above that. Overlaying from a baseline instead
    // would hide the amber whenever the lows are louder, which is most of the
    // time.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([75, 45, 30]), 1, 150, "overview", true);
    expect(fills).toHaveLength(3);

    // Each sits directly on the one below, and the lowest sits on the floor.
    expect(fills[0]!.y + fills[0]!.h).toBeCloseTo(150, 5);
    expect(fills[1]!.y + fills[1]!.h).toBeCloseTo(fills[0]!.y, 5);
    expect(fills[2]!.y + fills[2]!.h).toBeCloseTo(fills[1]!.y, 5);

    // Blue at the bottom, then amber, then near-white.
    expect(fills.map((f) => f.style)).toEqual([
      ramp(bandStops("overview"), 0),
      ramp(bandStops("overview"), 0.5),
      ramp(bandStops("overview"), 1),
    ]);
  });

  it("weighs the three bands the way rekordbox paints them", () => {
    // Not one scale for all three. Matched column for column against a 2x
    // capture, the blue rises v/128 of the band, the amber v/256 and the
    // near-white v/128. Dividing all three by a flat 150 drew the amber thin
    // and capped every loud passage in white.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([32, 32, 32]), 1, 256, "overview", true);
    expect(fills.map((f) => f.h)).toEqual([64, 32, 64]);
  });

  it("never draws a stacked column past the top of the cell", () => {
    // The bands do not peak together, so the stack scale is well under three
    // times the band scale — a loud column clips rather than overflowing.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([127, 127, 127]), 1, 40, "overview", true);
    for (const fill of fills) {
      expect(fill.y).toBeGreaterThanOrEqual(0);
      expect(fill.y + fill.h).toBeLessThanOrEqual(40.001);
    }
  });

  it("draws nothing for a silent column, and survives an empty tag", () => {
    const silent = recorder();
    drawBands(silent.ctx, new Uint8Array([0, 0, 0]), 1, 100);
    expect(silent.fills).toHaveLength(0);

    const empty = recorder();
    expect(() => drawBands(empty.ctx, new Uint8Array(), 10, 10)).not.toThrow();
    // A trailing partial entry must not be read as a column.
    expect(() => drawBands(empty.ctx, new Uint8Array([1, 2]), 10, 10)).not.toThrow();
    expect(empty.fills).toHaveLength(0);
  });

  it("leaves the measured margin clear at the top and bottom", () => {
    // rekordbox's detail waveform paints y 374..647 inside a band running
    // 358..650: the strip above carries the bar count and the cue heads.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([127, 0, 0]), 1, 100, "detail", false, { top: 8, bottom: 2 });
    const fill = fills[0]!;
    expect(fill.y).toBeCloseTo(8, 5);
    expect(fill.y + fill.h).toBeCloseTo(98, 5);
  });

  it("insets a stacked half waveform from the same edges", () => {
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([150, 0, 0]), 1, 100, "overview", true, { top: 8, bottom: 2 });
    const fill = fills[0]!;
    expect(fill.y + fill.h).toBeCloseTo(98, 5);
    expect(fill.y).toBeGreaterThanOrEqual(8);
  });

  it("still fills the band when no inset is asked for", () => {
    // The row preview passes none, and must keep every pixel of a 25px row.
    const { ctx, fills } = recorder();
    drawBands(ctx, new Uint8Array([150, 0, 0]), 1, 25, "overview", true);
    expect(fills[0]!.y + fills[0]!.h).toBeCloseTo(25, 5);
  });

  it("survives an inset taller than the strip", () => {
    const { ctx, fills } = recorder();
    expect(() =>
      drawBands(ctx, new Uint8Array([127, 0, 0]), 1, 6, "detail", false, { top: 40, bottom: 40 }),
    ).not.toThrow();
    for (const fill of fills) {
      expect(fill.y).toBeGreaterThanOrEqual(0);
      expect(fill.y + fill.h).toBeLessThanOrEqual(6.001);
    }
  });

  it("takes the loudest column when many share a pixel", () => {
    // Four columns into one pixel: the peak must survive, or a transient
    // vanishes at overview width.
    const { ctx, fills } = recorder();
    const data = new Uint8Array([1, 0, 0, 127, 0, 0, 1, 0, 0, 1, 0, 0]);
    drawBands(ctx, data, 1, 100, "overview");
    expect(fills[0]!.h).toBeCloseTo(100, 5);
  });
});
