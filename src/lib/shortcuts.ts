/**
 * The keyboard map, as a pure function.
 *
 * Kept apart from the components so the whole map is testable without a DOM,
 * and so the platform difference lives in one place: macOS uses Command where
 * Windows uses Control, and getting that wrong makes every shortcut either
 * dead or triggered by accident.
 */

/** What a key press asks for. */
export type Action =
  | "focusSearch"
  | "clearSearch"
  // A: the selected tracks go to the analyser, as rekordbox's Browse preset binds it.
  | "analyseSelection"
  | "selectAll"
  | "clearSelection"
  | "moveUp"
  | "moveDown"
  | "extendUp"
  | "extendDown"
  | "pageUp"
  | "pageDown"
  | "toTop"
  | "toBottom"
  // Enter loads the highlighted track onto Player 1 (deck A), wherever the
  // focus is in the browser — the keyboard counterpart of double-clicking it.
  | "loadPlayer1"
  // The deck. Every key below is rekordbox's own, transcribed from the Export
  // preset in `KeyMappings/rekordbox_0000000000030.mappings` — the key map the
  // mode we clone ships with, not a guess at what feels natural.
  | "playPause"
  | "cue"
  | "quantize"
  | "jumpBack"
  | "jumpForward"
  | "showMemory"
  | "showHotCues"
  | "showInfo"
  // The MEMORY cluster: M stores the cue point as a memory cue, B and N call
  // the one before and after the playhead, X deletes the one it is on.
  | "memoryCue"
  | "previousMemoryCue"
  | "nextMemoryCue"
  | "deleteMemoryCue"
  // The hot cue pads: the Export preset binds `1`, `2` and `3` to `Set Hot
  // Cue A` to `C` and `command + 1`-`3` to `Clear Hot Cue A` to `C`, and
  // nothing to D onwards.
  | "hotCueA"
  | "hotCueB"
  | "hotCueC"
  | "clearHotCueA"
  | "clearHotCueB"
  | "clearHotCueC";

/**
 * The pad a hot cue action names, and whether it clears rather than sets.
 * `null` for any other action.
 */
export function hotCuePad(action: Action): { letter: string; clear: boolean } | null {
  const set = /^hotCue([A-C])$/.exec(action);
  if (set) return { letter: set[1] ?? "", clear: false };
  const clear = /^clearHotCue([A-C])$/.exec(action);
  if (clear) return { letter: clear[1] ?? "", clear: true };
  return null;
}

/** The parts of a keyboard event the map reads. */
export interface KeyChord {
  key: string;
  /** Command on macOS. */
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}

export interface Platform {
  /** True on macOS, where Command is the modifier rather than Control. */
  mac: boolean;
}

/** Whether the platform's primary modifier is held, and only it. */
function primary(chord: KeyChord, platform: Platform): boolean {
  return platform.mac ? chord.metaKey === true : chord.ctrlKey === true;
}

/**
 * The native menu item a chord is the accelerator of, on a platform where
 * the webview eats it — or `null`.
 *
 * On macOS the menu's key equivalents are handled by the application before
 * the webview sees a keystroke, so nothing is needed and nothing is
 * returned: a fallback there would fire the item twice. On Windows the
 * accelerators are bound too, but a keystroke that lands in the focused
 * webview never reaches them (Ctrl+, and Ctrl+8 did nothing on 0.5.1 while
 * typing into the search field proved the keys arrived), so the shell's ids
 * are produced here and handled exactly as a menu click is. Full screen is
 * left out: the shell does that one itself, on the native event.
 */
export function menuAccelerator(chord: KeyChord, platform: Platform): string | null {
  if (platform.mac || chord.ctrlKey !== true || chord.metaKey === true || chord.altKey === true) {
    return null;
  }
  if (chord.shiftKey === true) return null;
  switch (chord.key.toLowerCase()) {
    case ",":
      return "settings";
    case "o":
      return "import";
    case "i":
      return "info";
    case "b":
      return "sub";
    case "7":
      return "layout-one";
    case "8":
      return "layout-two";
    case "9":
      return "layout-simple";
    case "0":
      return "layout-browser";
    default:
      return null;
  }
}

/**
 * The action a chord asks for, or `null`.
 *
 * `null` means "not ours" — the caller must let the event through rather than
 * swallow it, or browser and OS shortcuts stop working inside the app.
 */
export function actionFor(chord: KeyChord, platform: Platform): Action | null {
  const mod = primary(chord, platform);
  // The other platform's modifier must not also trigger it, or Control-A on a
  // Mac would select all *and* move the caret to the start of the line.
  const wrongMod = platform.mac ? chord.ctrlKey === true : chord.metaKey === true;
  if (wrongMod) return null;

  if (mod && !chord.shiftKey && !chord.altKey) {
    switch (chord.key.toLowerCase()) {
      case "f":
        return "focusSearch";
      case "a":
        return "selectAll";
      case "1":
        return "clearHotCueA";
      case "2":
        return "clearHotCueB";
      case "3":
        return "clearHotCueC";
      default:
        break;
    }
  }
  if (mod && chord.key === "ArrowUp") return "toTop";
  if (mod && chord.key === "ArrowDown") return "toBottom";

  if (!mod && !chord.altKey) {
    // The deck's keys, unmodified, exactly as the Export preset binds them.
    switch (chord.key) {
      case " ":
        return "playPause";
      case "F10":
        return "showMemory";
      case "F11":
        return "showHotCues";
      case "F12":
        return "showInfo";
      case "ArrowLeft":
        return "jumpBack";
      case "ArrowRight":
        return "jumpForward";
      default:
        break;
    }
    switch (chord.key.toLowerCase()) {
      case "c":
        return "cue";
      case "q":
        return "quantize";
      case "m":
        return "memoryCue";
      case "b":
        return "previousMemoryCue";
      case "n":
        return "nextMemoryCue";
      case "x":
        return "deleteMemoryCue";
      case "a":
        return "analyseSelection";
      case "1":
        return "hotCueA";
      case "2":
        return "hotCueB";
      case "3":
        return "hotCueC";
      default:
        break;
    }
    switch (chord.key) {
      case "Enter":
        return "loadPlayer1";
      case "Escape":
        return "clearSearch";
      case "ArrowUp":
        return chord.shiftKey ? "extendUp" : "moveUp";
      case "ArrowDown":
        return chord.shiftKey ? "extendDown" : "moveDown";
      case "PageUp":
        return "pageUp";
      case "PageDown":
        return "pageDown";
      case "Home":
        return "toTop";
      case "End":
        return "toBottom";
      default:
        break;
    }
  }
  return null;
}

/**
 * Just enough of an element to decide whether it owns the keyboard.
 *
 * Structural rather than `HTMLElement`, so this module needs no DOM and stays
 * testable in the same plain-node environment as the rest of `src/lib`.
 */
export interface FocusTarget {
  tagName?: string;
  isContentEditable?: boolean;
}

/**
 * Whether a key press should be ignored because the user is typing into a
 * field.
 *
 * Arrow keys and Escape inside the search box belong to the box, not to the
 * track list — except the shortcut that focuses the box, which must still work
 * from anywhere.
 */
export function isTyping(target: FocusTarget | null | undefined): boolean {
  if (!target) return false;
  if (target.isContentEditable === true) return true;
  switch (target.tagName?.toUpperCase()) {
    case "INPUT":
    case "TEXTAREA":
    case "SELECT":
      return true;
    default:
      return false;
  }
}

/** Actions that still apply while a field has focus. */
const WHILE_TYPING: ReadonlySet<Action> = new Set<Action>([
  "focusSearch",
  "clearSearch",
]);

/** The action to run for an event, accounting for where the focus is. */
export function dispatch(
  chord: KeyChord,
  platform: Platform,
  target: FocusTarget | null | undefined,
): Action | null {
  const action = actionFor(chord, platform);
  if (action === null) return null;
  if (isTyping(target) && !WHILE_TYPING.has(action)) return null;
  return action;
}

/**
 * The bindings, for the Preferences window's Keyboard pane.
 *
 * The same map as `actionFor`, written out: what each key does, in
 * rekordbox's own wording and grouping (Browse, Player A) from the Export
 * preset, plus the menu accelerators the shell binds. Read-only — the
 * preset is transcribed, not edited.
 */
export interface Binding {
  group: "Browse" | "Player A" | "Menu";
  /** rekordbox's description of the command. */
  label: string;
  chord: KeyChord;
  /**
   * rekordbox's command id for it in `keymap.ts`, when it is one of
   * rekordbox's: the Keyboard pane draws that row live. A binding without
   * one is this app's own, listed under `pane`.
   */
  command?: string;
  /** Where the Keyboard pane files a binding that is this app's own. */
  pane?: "Browse" | "View" | "Track" | "File";
}

export const BINDINGS: readonly Binding[] = [
  { group: "Browse", label: "Search", chord: { key: "f", metaKey: true }, command: "7003" },
  { group: "Browse", label: "Select All", chord: { key: "a", metaKey: true }, pane: "Browse" },
  { group: "Browse", label: "Cursor to Top", chord: { key: "Home" }, pane: "Browse" },
  { group: "Browse", label: "Cursor to Bottom", chord: { key: "End" }, pane: "Browse" },
  { group: "Browse", label: "Analyze Track", chord: { key: "a" }, pane: "Browse" },
  { group: "Browse", label: "Load on Player 1", chord: { key: "Enter" }, pane: "Browse" },
  { group: "Player A", label: "Play/Pause", chord: { key: " " }, command: "3006" },
  { group: "Player A", label: "Quantize", chord: { key: "q" }, command: "301c" },
  { group: "Player A", label: "Cue", chord: { key: "c" }, command: "3007" },
  { group: "Player A", label: "Jump Reverse", chord: { key: "ArrowLeft" }, command: "3009" },
  { group: "Player A", label: "Jump Forward", chord: { key: "ArrowRight" }, command: "3008" },
  { group: "Player A", label: "Memory Cue", chord: { key: "m" }, command: "3024" },
  { group: "Player A", label: "Call Previous Memory Cue", chord: { key: "b" }, command: "303a" },
  { group: "Player A", label: "Call Next Memory Cue", chord: { key: "n" }, command: "3039" },
  { group: "Player A", label: "Delete Memory Cue", chord: { key: "x" }, command: "303b" },
  { group: "Player A", label: "Set Hot Cue A", chord: { key: "1" }, command: "301e" },
  { group: "Player A", label: "Set Hot Cue B", chord: { key: "2" }, command: "301f" },
  { group: "Player A", label: "Set Hot Cue C", chord: { key: "3" }, command: "3020" },
  { group: "Player A", label: "Clear Hot Cue A", chord: { key: "1", metaKey: true }, command: "3021" },
  { group: "Player A", label: "Clear Hot Cue B", chord: { key: "2", metaKey: true }, command: "3022" },
  { group: "Player A", label: "Clear Hot Cue C", chord: { key: "3", metaKey: true }, command: "3023" },
  { group: "Player A", label: "Show Memory Cues", chord: { key: "F10" }, command: "303f" },
  { group: "Player A", label: "Show Hot Cues", chord: { key: "F11" }, command: "3040" },
  { group: "Player A", label: "Show Information", chord: { key: "F12" }, command: "3041" },
  { group: "Menu", label: "Import File", chord: { key: "o", metaKey: true }, command: "2000" },
  { group: "Menu", label: "Preferences", chord: { key: ",", metaKey: true }, command: "200a" },
  { group: "Menu", label: "Information Window", chord: { key: "i", metaKey: true }, command: "b103" },
  { group: "Menu", label: "Sub Browser", chord: { key: "b", metaKey: true }, pane: "View" },
  { group: "Menu", label: "1 Player", chord: { key: "7", metaKey: true }, command: "b040" },
  { group: "Menu", label: "2 Players", chord: { key: "8", metaKey: true }, command: "b043" },
  { group: "Menu", label: "Simple Player", chord: { key: "9", metaKey: true }, command: "b041" },
  { group: "Menu", label: "Full Browser", chord: { key: "0", metaKey: true }, command: "b042" },
  { group: "Menu", label: "Full Screen", chord: { key: "f", metaKey: true, shiftKey: true }, command: "b04e" },
];

/** The names rekordbox prints in a key badge for keys that are not letters. */
const KEY_NAMES: Record<string, string> = {
  " ": "spacebar",
  Enter: "enter",
  ArrowLeft: "cursor left",
  ArrowRight: "cursor right",
  ArrowUp: "cursor up",
  ArrowDown: "cursor down",
  Home: "home",
  End: "end",
  Escape: "esc",
};

/**
 * A chord as rekordbox's Keyboard pane prints it: `spacebar`,
 * `command + cursor down`, `shift + command + F`. `metaKey` in a binding
 * means the platform's primary modifier, so it reads `ctrl` on Windows.
 */
export function describeChord(chord: KeyChord, platform: Platform): string {
  const parts: string[] = [];
  if (chord.shiftKey) parts.push("shift");
  if (chord.altKey) parts.push(platform.mac ? "option" : "alt");
  if (chord.metaKey || chord.ctrlKey) parts.push(platform.mac ? "command" : "ctrl");
  const name = KEY_NAMES[chord.key] ?? (chord.key.length === 1 ? chord.key.toUpperCase() : chord.key);
  parts.push(name);
  return parts.join(" + ");
}

/** The running platform, read once. */
export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return { mac: false };
  // `platform` is deprecated but is the only reliable signal in WKWebView;
  // userAgent carries "Macintosh" there too, so either answers.
  const hint = `${navigator.platform ?? ""} ${navigator.userAgent}`;
  return { mac: /Mac|iPhone|iPad/.test(hint) };
}
