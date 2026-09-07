import { describe, expect, it } from "vitest";
import { applyClick, emptySelection, modifierFor } from "./selection";

describe("selection", () => {
  it("reads the modifier from the event", () => {
    expect(modifierFor({ shiftKey: false, metaKey: false, ctrlKey: false })).toBe("none");
    expect(modifierFor({ shiftKey: false, metaKey: true, ctrlKey: false })).toBe("toggle");
    expect(modifierFor({ shiftKey: false, metaKey: false, ctrlKey: true })).toBe("toggle");
    expect(modifierFor({ shiftKey: true, metaKey: true, ctrlKey: false })).toBe("range");
  });

  it("a plain click replaces the selection and moves the anchor", () => {
    const s = applyClick(emptySelection, { id: "a", index: 3 }, "none");
    expect([...s.ids]).toEqual(["a"]);
    expect(s.anchorIndex).toBe(3);
  });

  it("toggle adds and removes", () => {
    let s = applyClick(emptySelection, { id: "a", index: 0 }, "none");
    s = applyClick(s, { id: "b", index: 1 }, "toggle");
    expect([...s.ids].sort()).toEqual(["a", "b"]);
    s = applyClick(s, { id: "a", index: 0 }, "toggle");
    expect([...s.ids]).toEqual(["b"]);
  });

  it("range uses the ids the backend resolved and keeps the anchor put", () => {
    let s = applyClick(emptySelection, { id: "a", index: 2 }, "none");
    s = applyClick(s, { id: "e", index: 6 }, "range", ["a", "b", "c", "d", "e"]);
    expect(s.ids.size).toBe(5);
    expect(s.anchorIndex).toBe(2);
    // A second shift-click grows from the same origin, not from the last click.
    s = applyClick(s, { id: "c", index: 4 }, "range", ["a", "b", "c"]);
    expect(s.anchorIndex).toBe(2);
    expect(s.ids.size).toBe(3);
  });

  it("range without an anchor degrades to a plain click", () => {
    const s = applyClick(emptySelection, { id: "z", index: 9 }, "range", undefined);
    expect([...s.ids]).toEqual(["z"]);
  });
});
