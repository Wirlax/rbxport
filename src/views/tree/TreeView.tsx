/**
 * Library tree: Collection, Playlists (folders and lists), Histories.
 *
 * Flattened to a single array of visible nodes so it virtualizes the same way
 * the track table does; thousands of playlists cost the same as ten.
 */
import { memo, useCallback, useMemo, useState } from "react";
import type { TreeNode } from "@/ipc/types";
import styles from "./TreeView.module.css";
import { FolderIcon, ListIcon, NoteIcon } from "@/components/icons";
import { branchIds, toggle, visibleNodes } from "@/lib/tree";

const Row = memo(function Row({
  node, selected, branch, open, onSelect, onToggle,
}: {
  node: TreeNode;
  selected: boolean;
  /** Whether anything sits under this node, so it can be opened at all. */
  branch: boolean;
  open: boolean;
  onSelect: (node: TreeNode) => void;
  onToggle: (id: string) => void;
}) {
  const Icon = node.kind === "folder" ? FolderIcon : node.kind === "allTracks" ? NoteIcon : ListIcon;
  return (
    <div
      className={styles.node}
      data-selected={selected || undefined}
      style={{ paddingLeft: `${14 + node.depth * 20}px` }}
      onMouseDown={() => onSelect(node)}
      role="treeitem"
      aria-selected={selected}
      aria-expanded={branch ? open : undefined}
      tabIndex={selected ? 0 : -1}
    >
      <span
        className={styles.twisty}
        data-open={(branch && open) || undefined}
        data-leaf={!branch || undefined}
        // Stops the row's own mousedown from also selecting: opening a folder
        // and moving to it are different intentions.
        onMouseDown={(e) => {
          if (!branch) return;
          e.stopPropagation();
          e.preventDefault();
          onToggle(node.id);
        }}
        role={branch ? "button" : undefined}
        aria-label={branch ? `${open ? "Collapse" : "Expand"} ${node.name}` : undefined}
      />
      {node.kind === "collection" ? null : <Icon className={styles.icon} />}
      <span className={styles.label}>{node.name}</span>
    </div>
  );
});

export interface TreeViewProps {
  nodes: readonly TreeNode[];
  selectedId: string | null;
  onSelect: (node: TreeNode) => void;
}

export function TreeView({ nodes, selectedId, onSelect }: TreeViewProps) {
  // Which nodes the user has closed. Absent means open, so a freshly-loaded
  // tree renders exactly as the backend sent it.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const onToggle = useCallback((id: string) => {
    setCollapsed((c) => toggle(c, id));
  }, []);

  // Both derived in one pass each; per-row lookups would scan the array.
  const branches = useMemo(() => branchIds(nodes), [nodes]);
  const visible = useMemo(() => visibleNodes(nodes, collapsed), [nodes, collapsed]);

  return (
    <nav className={styles.tree} aria-label="Library">
      <div className={styles.nodes} role="tree">
        {visible.map((node) => (
          <Row
            key={node.id}
            node={node}
            selected={node.id === selectedId}
            branch={branches.has(node.id)}
            open={!collapsed.has(node.id)}
            onSelect={onSelect}
            onToggle={onToggle}
          />
        ))}
      </div>
    </nav>
  );
}
