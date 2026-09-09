/**
 * Which tree nodes are visible, given what the user has collapsed.
 *
 * The backend hands back one flat array with a `depth` per node, which is what
 * lets the tree virtualize the same way the track table does. Collapsing has
 * to work on that flat shape rather than on a nested one, so this is a single
 * pass: once a node is collapsed, everything deeper than it is skipped until
 * the depth comes back up.
 */
import type { TreeNode } from "@/ipc/types";

/** A node can be collapsed only if something sits under it. */
export function hasChildren(nodes: readonly TreeNode[], index: number): boolean {
  const node = nodes[index];
  const next = nodes[index + 1];
  return node !== undefined && next !== undefined && next.depth > node.depth;
}

/**
 * The ids of every node with something under it, in one pass.
 *
 * Asking `hasChildren` per rendered row means finding that row's index first,
 * which is a scan — O(n²) over the whole tree, and this library has 683
 * playlists.
 */
export function branchIds(nodes: readonly TreeNode[]): Set<string> {
  const out = new Set<string>();
  for (let i = 0; i < nodes.length; i++) {
    if (hasChildren(nodes, i)) {
      const node = nodes[i];
      if (node) out.add(node.id);
    }
  }
  return out;
}

/**
 * The nodes to render, in order, with everything under a collapsed node
 * removed.
 *
 * `collapsed` holds ids the user has closed. A node missing from it is open,
 * so a tree with nothing collapsed renders whole — which is what the backend
 * hands back.
 */
export function visibleNodes(
  nodes: readonly TreeNode[],
  collapsed: ReadonlySet<string>,
): TreeNode[] {
  const out: TreeNode[] = [];
  // The depth below which nodes are hidden, or null when nothing is hidden.
  let hiddenBelow: number | null = null;

  for (const node of nodes) {
    if (hiddenBelow !== null) {
      if (node.depth > hiddenBelow) continue;
      // Back up to the collapsed node's level or shallower: visible again.
      hiddenBelow = null;
    }
    out.push(node);
    if (collapsed.has(node.id)) hiddenBelow = node.depth;
  }
  return out;
}

/** Adds or removes an id, returning a new set so React sees the change. */
export function toggle(collapsed: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(collapsed);
  if (!next.delete(id)) next.add(id);
  return next;
}

/**
 * Which part of the library the tree is showing.
 *
 * No Collection: the tree opens with All Tracks at the top and always in
 * sight, so a button whose whole job is to scroll to the first row is a
 * shortcut to where you already are.
 */
export type Source = "playlists" | "histories" | "devices";

/**
 * The nodes belonging to one source.
 *
 * The backend sends one flat tree covering every section, so the rail narrows
 * it here rather than asking for a different tree — switching sections is then
 * instant and costs no round trip.
 */
export function nodesForSource(nodes: readonly TreeNode[], source: Source): TreeNode[] {
  switch (source) {
    case "histories":
      return nodes.filter((n) => n.kind === "history");
    case "devices":
      return nodes.filter((n) => n.kind === "device");
    case "playlists":
      return nodes.filter((n) => n.kind === "folder" || n.kind === "playlist" || n.kind === "collection");
  }
}

/** Sources with nothing under them, so the rail can dim rather than hide them. */
export function emptySources(nodes: readonly TreeNode[]): Set<Source> {
  const empty = new Set<Source>();
  for (const source of ["playlists", "histories", "devices"] as const) {
    if (nodesForSource(nodes, source).length === 0) empty.add(source);
  }
  return empty;
}

/**
 * Which section a selected node belongs to, so the rail can show where you are.
 *
 * Defaults to playlists when nothing is selected, because that is what the
 * tree opens on — and All Tracks belongs to it too, since it sits at the top
 * of the same tree and the rail no longer has a button of its own for it.
 */
export function sourceOf(nodes: readonly TreeNode[], selectedId: string | null): Source {
  const node = nodes.find((n) => n.id === selectedId);
  switch (node?.kind) {
    case "history":
      return "histories";
    case "device":
      return "devices";
    default:
      return "playlists";
  }
}
