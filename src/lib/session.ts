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
import { asLayout, type PlayerLayout } from "./layout";
import type { RowDto, SortColumn, TreeNode } from "@/ipc/types";
import { DEFAULT_SORT, type SortState } from "./viewSpec";

/**
 * How many rows of the last view are kept.
 *
 * Enough to fill any window at the measured row pitch, and no more: this is
 * the first screen, not a second copy of the library.
 */
export const SEEDED_ROWS = 200;

/** A hard ceiling on the stored tree, so a pathological library cannot fill
 * localStorage. The reference collection has 683 playlists. */
export const SEEDED_NODES = 4000;

export interface Session {
  /** The library tree's width in pixels, before clamping to the window. */
  treeWidth: number;
  /** The tree node that was selected, if it still exists at load. */
  selectedNodeId: string | null;
  sort: SortState;
  infoOpen: boolean;
  subOpen: boolean;
  /**
   * The last screen, kept so the window can draw itself before the backend has
   * finished reading the library.
   *
   * This is a picture of what was there, not a source of truth: everything in
   * it is replaced the moment the real data arrives. A track edited elsewhere
   * shows its old value for the fraction of a second that takes, which is a
   * far better trade than an empty window for a second and a half.
   */
  tree: TreeNode[];
  rows: RowDto[];
  /** Row count of the view the rows came from, so the scrollbar is right. */
  count: number;
  /** What was loaded in the player. */
  player: RowDto | null;
  /** How much of the window the deck took: 1 player, 2, simple, or none. */
  layout: PlayerLayout;
}

export const DEFAULT_TREE_WIDTH = 305;

export const DEFAULT_SESSION: Session = {
  treeWidth: DEFAULT_TREE_WIDTH,
  selectedNodeId: null,
  sort: DEFAULT_SORT,
  infoOpen: false,
  subOpen: false,
  tree: [],
  rows: [],
  count: 0,
  player: null,
  layout: "one",
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

/** Keeps only entries that are objects carrying a string id. */
function records<T extends { id: string }>(value: unknown, limit: number): T[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (item): item is T =>
        typeof item === "object" && item !== null && typeof (item as T).id === "string",
    )
    .slice(0, limit);
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
    tree: records<TreeNode>(raw.tree, SEEDED_NODES),
    rows: records<RowDto>(raw.rows, SEEDED_ROWS),
    count:
      typeof raw.count === "number" && Number.isFinite(raw.count) && raw.count >= 0
        ? Math.round(raw.count)
        : 0,
    player:
      typeof raw.player === "object" &&
      raw.player !== null &&
      typeof (raw.player as RowDto).id === "string"
        ? (raw.player as RowDto)
        : null,
    layout: asLayout(raw.layout),
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
