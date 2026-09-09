import { describe, expect, it } from "vitest";

import { ARTWORK_RETRIES, ARTWORK_RETRY_MS, attemptUrl, retryDelay } from "./artwork";

describe("retryDelay", () => {
  it("backs off, doubling each time", () => {
    expect(retryDelay(0)).toBe(ARTWORK_RETRY_MS);
    expect(retryDelay(1)).toBe(ARTWORK_RETRY_MS * 2);
    expect(retryDelay(2)).toBe(ARTWORK_RETRY_MS * 4);
  });

  it("gives up rather than asking for ever", () => {
    // A file that is genuinely unreadable must stop costing requests, and the
    // tint behind it must be allowed to stand in.
    expect(retryDelay(ARTWORK_RETRIES)).toBeNull();
    expect(retryDelay(ARTWORK_RETRIES + 5)).toBeNull();
  });

  it("takes nothing but a whole count of attempts", () => {
    for (const bad of [-1, 0.5, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(retryDelay(bad)).toBeNull();
    }
  });

  it("covers the second the library takes to open", () => {
    let total = 0;
    for (let at = 0; at < ARTWORK_RETRIES; at++) total += retryDelay(at) ?? 0;
    expect(total).toBeGreaterThanOrEqual(1000);
  });
});

describe("attemptUrl", () => {
  it("asks for the plain source first", () => {
    expect(attemptUrl("rbl://artwork/7", 0)).toBe("rbl://artwork/7");
  });

  it("makes each retry a source the browser has not already failed on", () => {
    expect(attemptUrl("rbl://artwork/7", 1)).toBe("rbl://artwork/7?retry=1");
    expect(attemptUrl("rbl://artwork/7", 2)).toBe("rbl://artwork/7?retry=2");
  });
});
