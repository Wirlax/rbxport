/**
 * The view a tree selection asks the backend for.
 *
 * Held here because there is more than one browser now — the main one and the
 * sub-browser, each with its own selection — and two copies of this would
 * drift the moment a source kind was added.
 */
import type { SortColumn, TreeNode, ViewSpec } from "@/ipc/types";

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
): ViewSpec {
  const order = sort ?? DEFAULT_SORT;
  return {
    // Only a playlist narrows the view. A folder holds playlists rather than
    // tracks, and a device is not a track source at all.
    source: node?.kind === "playlist" ? { kind: "playlist", id: node.id } : { kind: "collection" },
    sort: order.column,
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
