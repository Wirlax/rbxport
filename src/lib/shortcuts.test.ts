import { describe, expect, it } from "vitest";

import { actionFor, dispatch, isTyping, type Platform } from "./shortcuts";

const MAC: Platform = { mac: true };
const WIN: Platform = { mac: false };

describe("actionFor", () => {
  it("uses Command on macOS and Control elsewhere", () => {
    expect(actionFor({ key: "f", metaKey: true }, MAC)).toBe("focusSearch");
    expect(actionFor({ key: "f", ctrlKey: true }, WIN)).toBe("focusSearch");
  });

  it("does not fire on the other platform's modifier", () => {
    // Control-A on a Mac moves the caret to the start of the line; taking it
    // over would break that everywhere in the app.
    expect(actionFor({ key: "a", ctrlKey: true }, MAC)).toBeNull();
    expect(actionFor({ key: "f", ctrlKey: true }, MAC)).toBeNull();
    expect(actionFor({ key: "a", metaKey: true }, WIN)).toBeNull();
  });

  it("does not fire when both modifiers are held", () => {
    expect(actionFor({ key: "a", metaKey: true, ctrlKey: true }, MAC)).toBeNull();
    expect(actionFor({ key: "a", metaKey: true, ctrlKey: true }, WIN)).toBeNull();
  });

  it("is not case sensitive about the letter", () => {
    // Shift is not held, but some layouts report an uppercase key anyway.
    expect(actionFor({ key: "F", metaKey: true }, MAC)).toBe("focusSearch");
    expect(actionFor({ key: "A", metaKey: true }, MAC)).toBe("selectAll");
  });

  it("maps the movement keys, and shift extends instead of moving", () => {
    expect(actionFor({ key: "ArrowUp" }, MAC)).toBe("moveUp");
    expect(actionFor({ key: "ArrowDown" }, MAC)).toBe("moveDown");
    expect(actionFor({ key: "ArrowUp", shiftKey: true }, MAC)).toBe("extendUp");
    expect(actionFor({ key: "ArrowDown", shiftKey: true }, MAC)).toBe("extendDown");
    expect(actionFor({ key: "PageUp" }, MAC)).toBe("pageUp");
    expect(actionFor({ key: "PageDown" }, MAC)).toBe("pageDown");
  });

  it("sends Home and End, and the modified arrows, to the ends of the list", () => {
    expect(actionFor({ key: "Home" }, MAC)).toBe("toTop");
    expect(actionFor({ key: "End" }, MAC)).toBe("toBottom");
    expect(actionFor({ key: "ArrowUp", metaKey: true }, MAC)).toBe("toTop");
    expect(actionFor({ key: "ArrowDown", metaKey: true }, MAC)).toBe("toBottom");
    expect(actionFor({ key: "ArrowUp", ctrlKey: true }, WIN)).toBe("toTop");
  });

  it("returns null for anything it does not claim", () => {
    // A claimed-but-unhandled key would be swallowed, and browser and OS
    // shortcuts would stop working inside the app. The arrows are claimed now
    // — rekordbox's Export map puts beat jump on them — so they are not here.
    for (const chord of [
      { key: "q", metaKey: true },
      { key: "r", metaKey: true },
      { key: "Tab" },
      { key: "z" },
      { key: "F5" },
      { key: "a", metaKey: true, altKey: true },
    ]) {
      expect(actionFor(chord, MAC)).toBeNull();
    }
  });
});

describe("dispatch", () => {
  const field = { tagName: "INPUT" };
  const list = { tagName: "DIV" };

  it("lets a field keep its own arrow keys and selection", () => {
    expect(dispatch({ key: "ArrowDown" }, MAC, field)).toBeNull();
    expect(dispatch({ key: "a", metaKey: true }, MAC, field)).toBeNull();
  });

  it("still focuses and clears the search from inside a field", () => {
    expect(dispatch({ key: "f", metaKey: true }, MAC, field)).toBe("focusSearch");
    expect(dispatch({ key: "Escape" }, MAC, field)).toBe("clearSearch");
  });

  it("applies every action outside a field", () => {
    expect(dispatch({ key: "ArrowDown" }, MAC, list)).toBe("moveDown");
    expect(dispatch({ key: "a", metaKey: true }, MAC, null)).toBe("selectAll");
  });
});

describe("isTyping", () => {
  it("recognises the elements that own their own keys", () => {
    for (const tagName of ["INPUT", "TEXTAREA", "SELECT", "input"]) {
      expect(isTyping({ tagName })).toBe(true);
    }
    expect(isTyping({ tagName: "DIV" })).toBe(false);
    expect(isTyping(null)).toBe(false);
    expect(isTyping(undefined)).toBe(false);
    expect(isTyping({})).toBe(false);
  });

  it("recognises a contenteditable element whatever its tag", () => {
    expect(isTyping({ tagName: "DIV", isContentEditable: true })).toBe(true);
  });
});

describe("rekordbox's own Export key map", () => {
  // Transcribed from `KeyMappings/rekordbox_0000000000030.mappings`, the key
  // map rekordbox ships for the mode this clones. The keys are its, not ours.
  const mac = { mac: true };

  it("gives the deck the keys rekordbox gives it", () => {
    expect(actionFor({ key: " " }, mac)).toBe("playPause");
    expect(actionFor({ key: "c" }, mac)).toBe("cue");
    expect(actionFor({ key: "q" }, mac)).toBe("quantize");
    expect(actionFor({ key: "ArrowLeft" }, mac)).toBe("jumpBack");
    expect(actionFor({ key: "ArrowRight" }, mac)).toBe("jumpForward");
  });

  it("gives the MEMORY cluster M, B, N and X", () => {
    // `M` Memory Cue, `B` Call Previous Memory Cue, `N` Call Next Memory
    // Cue, `X` Delete Memory Cue — the Export preset's own bindings.
    expect(actionFor({ key: "m" }, mac)).toBe("memoryCue");
    expect(actionFor({ key: "b" }, mac)).toBe("previousMemoryCue");
    expect(actionFor({ key: "n" }, mac)).toBe("nextMemoryCue");
    expect(actionFor({ key: "x" }, mac)).toBe("deleteMemoryCue");
    // ⌘X is cut, and ⌘M minimises the window.
    expect(actionFor({ key: "x", metaKey: true }, mac)).not.toBe("deleteMemoryCue");
    expect(actionFor({ key: "m", metaKey: true }, mac)).not.toBe("memoryCue");
    expect(dispatch({ key: "x" }, mac, { tagName: "INPUT" })).toBeNull();
  });

  it("puts the three cue lists on F10, F11 and F12", () => {
    expect(actionFor({ key: "F10" }, mac)).toBe("showMemory");
    expect(actionFor({ key: "F11" }, mac)).toBe("showHotCues");
    expect(actionFor({ key: "F12" }, mac)).toBe("showInfo");
  });

  it("leaves the deck's letters alone when a modifier is held", () => {
    // ⌘C is copy, and ⌘Q quits. Taking either would be a bug people notice at
    // the worst moment.
    expect(actionFor({ key: "c", metaKey: true }, mac)).not.toBe("cue");
    expect(actionFor({ key: "q", metaKey: true }, mac)).not.toBe("quantize");
  });

  it("does not fire the deck's letters into a search box", () => {
    expect(dispatch({ key: "c" }, mac, { tagName: "INPUT" })).toBeNull();
    expect(dispatch({ key: " " }, mac, { tagName: "INPUT" })).toBeNull();
  });
});
