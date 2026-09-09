import { describe, expect, it } from "vitest";

import { asLayout, deckCount, isFullDeck, isSideBySide, LAYOUTS, layoutLabel } from "./layout";

describe("deckCount", () => {
  it("counts the decks each layout draws", () => {
    expect(deckCount("one")).toBe(1);
    expect(deckCount("two")).toBe(2);
    expect(deckCount("dual")).toBe(2);
    expect(deckCount("simple")).toBe(1);
    expect(deckCount("browser")).toBe(0);
  });
});

describe("isFullDeck", () => {
  it("is the simple player that drops the pads and the cue list", () => {
    expect(isFullDeck("one")).toBe(true);
    expect(isFullDeck("two")).toBe(true);
    expect(isFullDeck("dual")).toBe(true);
    expect(isFullDeck("simple")).toBe(false);
  });
});

describe("isSideBySide", () => {
  it("is what tells the two two-deck layouts apart", () => {
    // The menu draws it: 2 PLAYER is stacked bars, DUAL two overlapping panels.
    expect(isSideBySide("dual")).toBe(true);
    expect(isSideBySide("two")).toBe(false);
    expect(isSideBySide("one")).toBe(false);
  });
});

describe("layoutLabel", () => {
  it("names every layout it offers", () => {
    for (const entry of LAYOUTS) expect(layoutLabel(entry.id)).toBe(entry.label);
  });
});

describe("asLayout", () => {
  it("takes back what it stored", () => {
    for (const entry of LAYOUTS) expect(asLayout(entry.id)).toBe(entry.id);
  });

  it("falls back rather than letting a bad value through", () => {
    // A layout read from a stored session two versions ago must not blank the
    // window; it opens on the one deck everything else assumes.
    for (const bad of [null, undefined, "", "three", 2, {}]) {
      expect(asLayout(bad)).toBe("one");
    }
  });
});
