/**
 * What the interface looked like when it was last closed.
 *
 * The window's own size and position are the shell's job — Tauri's
 * window-state plugin does that. This is everything inside it: which playlist
 * was open, how the panes were split, what it was sorted by, which panels were
 * showing.
 *
 * The search box is deliberately absent. Reopening to a filtered library you
 * did not ask for reads as a broken library, and the one keystroke to clear it
 * is not worth that.
 *
 * Everything stored is checked on the way back in. A playlist can be deleted
 * between runs, a stored width can come from a wider screen, and a hand-edited
 * value can be anything at all; none of those may produce a broken window.
 */
import type { SortColumn } from "@/ipc/types";
import { DEFAULT_SORT, type SortState } from "./viewSpec";

export interface Session {
  /** The library tree's width in pixels, before clamping to the window. */
  treeWidth: number;
  /** The tree node that was selected, if it still exists at load. */
  selectedNodeId: string | null;
  sort: SortState;
  infoOpen: boolean;
  subOpen: boolean;
}

export const DEFAULT_TREE_WIDTH = 305;

export const DEFAULT_SESSION: Session = {
  treeWidth: DEFAULT_TREE_WIDTH,
  selectedNodeId: null,
  sort: DEFAULT_SORT,
  infoOpen: false,
  subOpen: false,
};

const SORT_COLUMNS: readonly string[] = [
  "trackNo", "title", "artist", "album", "genre", "label",
  "bpm", "key", "duration", "rating", "dateAdded", "releaseDate",
];

function sortOrDefault(value: unknown): SortState {
  if (typeof value !== "object" || value === null) return DEFAULT_SORT;
  const raw = value as { column?: unknown; descending?: unknown };
  if (typeof raw.column !== "string" || !SORT_COLUMNS.includes(raw.column)) {
    return DEFAULT_SORT;
  }
  return { column: raw.column as SortColumn, descending: raw.descending === true };
}

/** Turns whatever was stored into a session that will render. */
export function sanitiseSession(value: unknown): Session {
  if (typeof value !== "object" || value === null) return DEFAULT_SESSION;
  const raw = value as Partial<Record<keyof Session, unknown>>;
  const width = raw.treeWidth;
  return {
    // Clamped again against the window at use; this only rejects nonsense.
    treeWidth:
      typeof width === "number" && Number.isFinite(width) && width > 0
        ? Math.round(width)
        : DEFAULT_TREE_WIDTH,
    selectedNodeId: typeof raw.selectedNodeId === "string" ? raw.selectedNodeId : null,
    sort: sortOrDefault(raw.sort),
    infoOpen: raw.infoOpen === true,
    subOpen: raw.subOpen === true,
  };
}

const KEY = "rbl.session";

/** Reads the stored session, falling back to the defaults on anything odd. */
export function loadSession(): Session {
  try {
    const raw = localStorage.getItem(KEY);
    return raw === null ? DEFAULT_SESSION : sanitiseSession(JSON.parse(raw));
  } catch {
    // A private window, cleared storage, or a browser that refuses it: the
    // defaults are a working interface, so there is nothing to report.
    return DEFAULT_SESSION;
  }
}

export function saveSession(session: Session): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(session));
  } catch {
    // Not being able to remember the layout is not worth interrupting anyone.
  }
}
