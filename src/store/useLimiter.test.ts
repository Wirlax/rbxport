import { describe, expect, it } from "vitest";

import { DEFAULT_LIMITER, sanitise } from "./useLimiter";

describe("sanitise", () => {
  it("gives the engine's defaults for nothing stored", () => {
    expect(sanitise(null)).toEqual(DEFAULT_LIMITER);
    expect(sanitise(undefined)).toEqual(DEFAULT_LIMITER);
  });

  it("keeps a stored value that is in range", () => {
    expect(sanitise({ enabled: false, ceilingDb: -3, releaseMs: 250 })).toEqual({
      enabled: false,
      ceilingDb: -3,
      releaseMs: 250,
    });
  });

  it("clamps what the engine would clamp, so the control shows the truth", () => {
    expect(sanitise({ enabled: true, ceilingDb: 4, releaseMs: 5 })).toEqual({
      enabled: true,
      ceilingDb: 0,
      releaseMs: 10,
    });
    expect(sanitise({ enabled: true, ceilingDb: -40, releaseMs: 9_000 }).ceilingDb).toBe(-12);
    expect(sanitise({ enabled: true, ceilingDb: -40, releaseMs: 9_000 }).releaseMs).toBe(1000);
  });

  it("falls back a field at a time on a hand-edited store", () => {
    expect(sanitise({ enabled: "yes", ceilingDb: "loud", releaseMs: Number.NaN })).toEqual(
      DEFAULT_LIMITER,
    );
    expect(sanitise({ releaseMs: 40 })).toEqual({ ...DEFAULT_LIMITER, releaseMs: 40 });
  });
});
