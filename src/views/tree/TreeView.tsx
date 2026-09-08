/**
 * Library tree: Collection, Playlists (folders and lists), Histories.
 *
 * Flattened to a single array of visible nodes so it virtualizes the same way
 * the track table does; thousands of playlists cost the same as ten.
 */
import { memo } from "react";
import type { TreeNode } from "@/ipc/types";
import styles from "./TreeView.module.css";
import { FolderIcon, ListIcon, NoteIcon } from "@/components/icons";

const Row = memo(function Row({
  node, selected, onSelect,
}: {
  node: TreeNode;
  selected: boolean;
  onSelect: (node: TreeNode) => void;
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
      aria-expanded={node.expanded}
      tabIndex={selected ? 0 : -1}
    >
      <span className={styles.twisty} data-open={node.expanded || undefined} data-leaf={node.expanded === undefined || undefined} />
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
  return (
    <nav className={styles.tree} aria-label="Library">
      <div className={styles.tabs}>
        <span className={styles.tab} data-active>Tree View</span>
        <span className={styles.tab}>Column View</span>
      </div>
      <div className={styles.nodes} role="tree">
        {nodes.map((node) => (
          <Row key={node.id} node={node} selected={node.id === selectedId} onSelect={onSelect} />
        ))}
      </div>
    </nav>
  );
}
