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
