import { describe, expect, it } from "vitest";

import {
  actionFor, BINDINGS, describeChord, dispatch, hotCuePad, isTyping, menuAccelerator, type Platform,
} from "./shortcuts";

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

  it("loads Player 1 on plain Enter, but leaves modified Enter alone", () => {
    expect(actionFor({ key: "Enter" }, MAC)).toBe("loadPlayer1");
    expect(actionFor({ key: "Enter", shiftKey: true }, MAC)).toBe("loadPlayer1");
    expect(actionFor({ key: "Enter", metaKey: true }, MAC)).toBeNull();
    expect(actionFor({ key: "Enter", ctrlKey: true }, WIN)).toBeNull();
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
    // Enter in the search box is the box's, not a load of Player 1.
    expect(dispatch({ key: "Enter" }, MAC, field)).toBeNull();
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

describe("menuAccelerator", () => {
  it("names the menu item a Windows accelerator stands for", () => {
    expect(menuAccelerator({ key: ",", ctrlKey: true }, WIN)).toBe("settings");
    expect(menuAccelerator({ key: "o", ctrlKey: true }, WIN)).toBe("import");
    expect(menuAccelerator({ key: "i", ctrlKey: true }, WIN)).toBe("info");
    expect(menuAccelerator({ key: "b", ctrlKey: true }, WIN)).toBe("sub");
    expect(menuAccelerator({ key: "7", ctrlKey: true }, WIN)).toBe("layout-one");
    expect(menuAccelerator({ key: "8", ctrlKey: true }, WIN)).toBe("layout-two");
    expect(menuAccelerator({ key: "9", ctrlKey: true }, WIN)).toBe("layout-simple");
    expect(menuAccelerator({ key: "0", ctrlKey: true }, WIN)).toBe("layout-browser");
  });

  it("does nothing on macOS, where the menu handles its own key equivalents", () => {
    expect(menuAccelerator({ key: ",", metaKey: true }, MAC)).toBeNull();
    expect(menuAccelerator({ key: ",", ctrlKey: true }, MAC)).toBeNull();
  });

  it("leaves other chords alone, full screen included", () => {
    expect(menuAccelerator({ key: "f", ctrlKey: true, shiftKey: true }, WIN)).toBeNull();
    expect(menuAccelerator({ key: ",", ctrlKey: true, altKey: true }, WIN)).toBeNull();
    expect(menuAccelerator({ key: "," }, WIN)).toBeNull();
    expect(menuAccelerator({ key: "x", ctrlKey: true }, WIN)).toBeNull();
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

  it("gives the first three pads 1, 2 and 3, and their clears the same with command", () => {
    // `Set Hot Cue A`-`C` on `1`-`3`, `Clear Hot Cue A`-`C` on `command + 1`-`3`.
    // The preset binds nothing past C, so 4 stays free.
    expect(actionFor({ key: "1" }, mac)).toBe("hotCueA");
    expect(actionFor({ key: "2" }, mac)).toBe("hotCueB");
    expect(actionFor({ key: "3" }, mac)).toBe("hotCueC");
    expect(actionFor({ key: "1", metaKey: true }, mac)).toBe("clearHotCueA");
    expect(actionFor({ key: "3", metaKey: true }, mac)).toBe("clearHotCueC");
    expect(actionFor({ key: "3", ctrlKey: true }, { mac: false })).toBe("clearHotCueC");
    expect(actionFor({ key: "4" }, mac)).toBeNull();
    expect(actionFor({ key: "4", metaKey: true }, mac)).toBeNull();
    // Typing a digit into the search box is typing.
    expect(dispatch({ key: "1" }, mac, { tagName: "INPUT" })).toBeNull();
    expect(hotCuePad("hotCueB")).toEqual({ letter: "B", clear: false });
    expect(hotCuePad("clearHotCueC")).toEqual({ letter: "C", clear: true });
    expect(hotCuePad("cue")).toBeNull();
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

describe("the Keyboard pane's bindings", () => {
  const mac: Platform = { mac: true };
  const windows: Platform = { mac: false };

  it("prints a chord the way rekordbox's badges do", () => {
    expect(describeChord({ key: " " }, mac)).toBe("spacebar");
    expect(describeChord({ key: "q" }, mac)).toBe("Q");
    expect(describeChord({ key: "ArrowDown", metaKey: true }, mac)).toBe("command + cursor down");
    expect(describeChord({ key: "ArrowDown", metaKey: true }, windows)).toBe("ctrl + cursor down");
    expect(describeChord({ key: "f", metaKey: true, shiftKey: true }, mac)).toBe("shift + command + F");
    expect(describeChord({ key: "F10" }, mac)).toBe("F10");
  });

  it("lists only chords the map actually answers to, under the deck and the browser", () => {
    // The menu accelerators are the shell's, and A is the track list's own
    // key for analysis; neither goes through the map.
    const mapped = BINDINGS.filter((b) => b.group !== "Menu" && b.label !== "Analyze Track");
    for (const binding of mapped) {
      const chord = { ...binding.chord, metaKey: binding.chord.metaKey ?? false };
      expect(actionFor(chord, mac), binding.label).not.toBeNull();
    }
  });
});
