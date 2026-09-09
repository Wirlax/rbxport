import { describe, expect, it } from "vitest";

import { extrapolate, follow, NO_ANCHOR, SNAP_SECONDS, type Anchor } from "./clock";

const anchor = (over: Partial<Anchor> = {}): Anchor => ({
  ...NO_ANCHOR,
  frames: 44_100,
  at: 1_000,
  sampleRate: 44_100,
  playing: true,
  ...over,
});

describe("extrapolate", () => {
  it("reads the frame counter as seconds", () => {
    expect(extrapolate(anchor({ playing: false }), 1_000)).toBeCloseTo(1);
  });

  it("runs on with real time while the deck plays", () => {
    // Half a second after a tick that said one second in.
    expect(extrapolate(anchor(), 1_500)).toBeCloseTo(1.5);
  });

  it("stands still while the deck is paused", () => {
    expect(extrapolate(anchor({ playing: false }), 5_000)).toBeCloseTo(1);
  });

  it("never runs backwards when a tick arrives late", () => {
    expect(extrapolate(anchor(), 900)).toBeCloseTo(1);
  });

  it("is zero before a device has said what rate it runs at", () => {
    expect(extrapolate(NO_ANCHOR, 10_000)).toBe(0);
  });

  it("follows the deck's own rate, for when tempo ships", () => {
    expect(extrapolate(anchor({ rate: 1.08 }), 2_000)).toBeCloseTo(1 + 1.08);
  });
});

describe("follow", () => {
  it("closes a small gap gradually rather than jumping", () => {
    const next = follow(1.0, 1.01, 16);
    expect(next).toBeGreaterThan(1.0);
    expect(next).toBeLessThan(1.01);
  });

  it("gets there: a gap left alone is gone within a third of a second", () => {
    let shown = 1.0;
    // Twenty frames at 60 Hz is 320 ms, or six time constants.
    for (let frame = 0; frame < 20; frame++) shown = follow(shown, 1.01, 16);
    expect(shown).toBeCloseTo(1.01, 4);
  });

  it("snaps rather than sliding across a seek", () => {
    // Sliding a whole second would draw the playhead crossing the track.
    expect(follow(1.0, 60.0, 16)).toBe(60.0);
    expect(follow(60.0, 1.0, 16)).toBe(1.0);
  });

  it("takes the target when no time has passed", () => {
    expect(follow(1.0, 1.05, 0)).toBe(1.05);
  });

  it("treats exactly the threshold as drift, not a seek", () => {
    expect(follow(1.0, 1.0 + SNAP_SECONDS, 16)).toBeLessThan(1.0 + SNAP_SECONDS);
  });
});
