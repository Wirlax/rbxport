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
  | "showInfo";

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
      default:
        break;
    }
    switch (chord.key) {
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

/** The running platform, read once. */
export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return { mac: false };
  // `platform` is deprecated but is the only reliable signal in WKWebView;
  // userAgent carries "Macintosh" there too, so either answers.
  const hint = `${navigator.platform ?? ""} ${navigator.userAgent}`;
  return { mac: /Mac|iPhone|iPad/.test(hint) };
}
