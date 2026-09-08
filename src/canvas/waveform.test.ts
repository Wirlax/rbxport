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

  it("scales the detail tag by its own full scale, not the overview's", () => {
    // PWV7 reaches about 127 and PWV6 about 63; using one scale for both draws
    // the detail at half height or clips it flat.
    const a = recorder();
    drawBands(a.ctx, new Uint8Array([127, 0, 0]), 1, 100, "detail");
    expect(a.fills[0]!.h).toBeCloseTo(100, 5);

    const b = recorder();
    drawBands(b.ctx, new Uint8Array([63, 0, 0]), 1, 100, "overview");
    expect(b.fills[0]!.h).toBeCloseTo(100, 5);
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

  it("takes the loudest column when many share a pixel", () => {
    // Four columns into one pixel: the peak must survive, or a transient
    // vanishes at overview width.
    const { ctx, fills } = recorder();
    const data = new Uint8Array([1, 0, 0, 63, 0, 0, 1, 0, 0, 1, 0, 0]);
    drawBands(ctx, data, 1, 100, "overview");
    expect(fills[0]!.h).toBeCloseTo(100, 5);
  });
});
