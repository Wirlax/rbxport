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

export const DEFAULT_SORT: SortState = { column: "title", descending: false };

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

/** Clicking a heading: ascending, then descending, then back to the default. */
export function nextSort(current: SortState, column: SortColumn): SortState {
  if (current.column !== column) return { column, descending: false };
  if (!current.descending) return { column, descending: true };
  return DEFAULT_SORT;
}
