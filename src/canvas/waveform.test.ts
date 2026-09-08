import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { bandStops, ramp } from "./waveform";

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
