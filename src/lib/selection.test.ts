import { describe, expect, it } from "vitest";
import { applyClick, clickSettles, emptySelection, modifierFor, pressSelects } from "./selection";

describe("selection", () => {
  it("reads the modifier from the event", () => {
    expect(modifierFor({ shiftKey: false, metaKey: false, ctrlKey: false })).toBe("none");
    expect(modifierFor({ shiftKey: false, metaKey: true, ctrlKey: false })).toBe("toggle");
    expect(modifierFor({ shiftKey: false, metaKey: false, ctrlKey: true })).toBe("toggle");
    expect(modifierFor({ shiftKey: true, metaKey: true, ctrlKey: false })).toBe("range");
  });

  it("a plain press on a selected row waits for the release; every other press applies", () => {
    const plain = { button: 0, shiftKey: false, metaKey: false, ctrlKey: false };
    // Selected and plain: this may be a drag of the selection.
    expect(pressSelects(plain, true)).toBe(false);
    expect(clickSettles(plain, true)).toBe(true);
    // Not selected: the press selects, and the click that follows changes nothing.
    expect(pressSelects(plain, false)).toBe(true);
    expect(clickSettles(plain, false)).toBe(false);
    // Modified presses are never drags and apply at once.
    expect(pressSelects({ ...plain, shiftKey: true }, true)).toBe(true);
    expect(pressSelects({ ...plain, metaKey: true }, true)).toBe(true);
    expect(clickSettles({ ...plain, metaKey: true }, true)).toBe(false);
  });

  it("the right button keeps a selection it lands in, and selects outside it", () => {
    const right = { button: 2, shiftKey: false, metaKey: false, ctrlKey: false };
    expect(pressSelects(right, true)).toBe(false);
    expect(clickSettles(right, true)).toBe(false);
    expect(pressSelects(right, false)).toBe(true);
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
