/**
 * Library tree: Collection, Playlists (folders and lists), Histories.
 *
 * Flattened to a single array of visible nodes so it virtualizes the same way
 * the track table does; thousands of playlists cost the same as ten.
 */
import { memo, useCallback, useMemo, useState } from "react";
import type { TreeNode } from "@/ipc/types";
import styles from "./TreeView.module.css";
import { DeviceIcon, FolderIcon, ListIcon, NoteIcon } from "@/components/icons";
import { ContextMenu } from "@/components/ContextMenu";
import { treeMenu } from "@/lib/contextMenus";
import {
  branchIds, emptySources, nodesForSource, sourceOf, toggle, visibleNodes, type Source,
} from "@/lib/tree";
import { SourceRail } from "./SourceRail";

const Row = memo(function Row({
  node, selected, branch, open, onSelect, onToggle, droppable, onDropTracks, onMenu,
}: {
  node: TreeNode;
  selected: boolean;
  /** Whether a track drag could land here. */
  droppable: boolean;
  onDropTracks: ((playlistId: string) => void) | undefined;
  onMenu: ((node: TreeNode, at: { x: number; y: number }) => void) | undefined;
  /** Whether anything sits under this node, so it can be opened at all. */
  branch: boolean;
  open: boolean;
  onSelect: (node: TreeNode) => void;
  onToggle: (id: string) => void;
}) {
  const Icon =
    node.kind === "folder"
      ? FolderIcon
      : node.kind === "allTracks"
        ? NoteIcon
        : node.kind === "device"
          ? DeviceIcon
          : ListIcon;
  return (
    <div
      className={styles.node}
      data-selected={selected || undefined}
      style={{ paddingLeft: `${14 + node.depth * 20}px` }}
      onMouseDown={() => onSelect(node)}
      onDragOver={(e) => {
        // Only a playlist takes tracks: a folder holds playlists, and dropping
        // into one would have to invent which.
        if (!droppable) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
      }}
      onDrop={(e) => {
        if (!droppable) return;
        e.preventDefault();
        onDropTracks?.(node.id);
      }}
      onContextMenu={(e) => {
        // Only the two kinds that have a menu: the fixed roots and the device
        // nodes are not playlists and have nothing to offer.
        if ((node.kind !== "playlist" && node.kind !== "folder") || !onMenu) return;
        e.preventDefault();
        onMenu(node, { x: e.clientX, y: e.clientY });
      }}
      data-droppable={droppable || undefined}
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
  /** Write a playlist to a stick. */
  onExport?: (node: TreeNode) => void;
  /** Create, delete and rename, which the shell owns because they write. */
  onCreatePlaylist?: (parent: TreeNode) => void;
  onCreateFolder?: (parent: TreeNode) => void;
  onDeleteNode?: (node: TreeNode) => void;
  /** rekordbox is running, so every write is greyed rather than raced. */
  readOnly?: boolean;
  /** True while tracks are being dragged, so playlists can offer themselves. */
  dragging?: boolean;
  /** Drop the dragged tracks onto a playlist. */
  onDropTracks?: (playlistId: string) => void;
}

export function TreeView({
  nodes, selectedId, onSelect, dragging, onDropTracks, onExport,
  onCreatePlaylist, onCreateFolder, onDeleteNode, readOnly = false,
}: TreeViewProps) {
  /** The tree menu: where it is, and which node it was opened on. */
  const [menu, setMenu] = useState<{ x: number; y: number; node: TreeNode } | null>(null);
  // Which nodes the user has closed. Absent means open, so a freshly-loaded
  // tree renders exactly as the backend sent it.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());

  const onToggle = useCallback((id: string) => {
    setCollapsed((c) => toggle(c, id));
  }, []);

  const empty = useMemo(() => emptySources(nodes), [nodes]);
  // Both derived in one pass each; per-row lookups would scan the array.
  const branches = useMemo(() => branchIds(nodes), [nodes]);
  const visible = useMemo(() => visibleNodes(nodes, collapsed), [nodes, collapsed]);

  // The rail is a shortcut, not a filter — `browseSetting.xml` calls it
  // TreeShortcut. rekordbox keeps one tree and jumps to a section; filtering
  // instead would hide Collection whenever Playlists was picked.
  const source = useMemo(() => sourceOf(nodes, selectedId), [nodes, selectedId]);
  const jumpTo = useCallback(
    (wanted: Source) => {
      const first = nodesForSource(nodes, wanted)[0];
      if (first) onSelect(first);
    },
    [nodes, onSelect],
  );

  return (
    <nav className={styles.tree} aria-label="Library">
      <SourceRail selected={source} onSelect={jumpTo} empty={empty} />
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
            droppable={Boolean(dragging) && node.kind === "playlist"}
            onDropTracks={onDropTracks}
            onMenu={(node, at) => setMenu({ ...at, node })}
          />
        ))}
        {visible.length === 0 ? (
          <p className={styles.emptyNote}>Nothing here yet.</p>
        ) : null}
      </div>

      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          rows={treeMenu(menu.node.kind === "folder" ? "folder" : "playlist")}
          label={menu.node.kind === "folder" ? "Folder" : "Playlist"}
          context={{ inPlaylist: true, hasFile: true, readOnly }}
          onChoose={(action) => {
            switch (action) {
              case "export":
                onExport?.(menu.node);
                break;
              case "createPlaylist":
                onCreatePlaylist?.(menu.node);
                break;
              case "createFolder":
                onCreateFolder?.(menu.node);
                break;
              case "delete":
                onDeleteNode?.(menu.node);
                break;
              default:
                break;
            }
          }}
          onClose={() => setMenu(null)}
        />
      ) : null}
    </nav>
  );
}
