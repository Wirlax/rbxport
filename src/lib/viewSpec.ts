/**
 * The view a tree selection asks the backend for.
 *
 * Held here because there is more than one browser now — the main one and the
 * sub-browser, each with its own selection — and two copies of this would
 * drift the moment a source kind was added.
 */
import type { KeyDisplay, SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { explorerPath } from "./explorer";

export interface SortState {
  column: SortColumn;
  descending: boolean;
}

/**
 * The view's own order: a playlist's membership, the collection's rows.
 *
 * `trackNo` is not a column the backend ranks by — it means "leave the rows in
 * the order this view produced them". That is what makes it the third state of
 * the sort cycle, and what a playlist has to open in: sorting a playlist by
 * anything at all destroys the order somebody put it in.
 */
export const NO_SORT: SortState = { column: "trackNo", descending: false };

/** What a view opens with, which is its own order. */
export const DEFAULT_SORT: SortState = NO_SORT;

/** The spec for a selected node, or the whole collection when none is. */
export function specForNode(
  node: TreeNode | null,
  query: string,
  sort: SortState | null,
  /**
   * How the Key column is shown. The column sorts by what it shows, as
   * rekordbox's does: classic names alphabetically, Camelot codes round the
   * wheel — `Abm` is `1A`, and alphabetical on the names it would come
   * after `A` and `Ab`.
   */
  keyDisplay: KeyDisplay = "classic",
): ViewSpec {
  const order = sort ?? DEFAULT_SORT;
  return {
    // A playlist or a history session narrows the view. A folder holds lists
    // rather than tracks — including a history year or month, which opens
    // empty because that is what it holds — and a device is not a track source
    // at all.
    // The Explorer's folders open as themselves, and its heading as an empty
    // folder: rekordbox shows an Explorer with nothing in it there.
    source:
      node?.kind === "playlist"
        ? { kind: "playlist", id: node.id }
        : node?.kind === "history"
          ? { kind: "history", id: node.id }
          : node?.kind === "directory"
            ? { kind: "folder", path: explorerPath(node.id) ?? "" }
            : node?.kind === "explorer"
              ? { kind: "folder", path: "" }
              : { kind: "collection" },
    sort: order.column === "key" && keyDisplay === "alphanumeric" ? "keyCamelot" : order.column,
    descending: order.descending,
    query,
  };
}

/**
 * Clicking a heading cycles ascending, descending, off.
 *
 * "Off" is the view's own order rather than another column's: a playlist that
 * could not be put back the way it was would make sorting it a one-way door.
 */
export function nextSort(current: SortState, column: SortColumn): SortState {
  if (current.column !== column) return { column, descending: false };
  if (!current.descending) return { column, descending: true };
  return NO_SORT;
}
