/**
 * Library tree: Collection, Playlists (folders and lists), Histories, Devices,
 * and the Explorer's folders.
 *
 * Flattened to a single array of visible nodes so it virtualizes the same way
 * the track table does; thousands of playlists cost the same as ten.
 */
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { TreeNode } from "@/ipc/types";
import styles from "./TreeView.module.css";
import { DeviceIcon, FolderIcon, HistoryIcon, ListIcon, NoteIcon } from "@/components/icons";
import { ContextMenu } from "@/components/ContextMenu";
import { treeMenu } from "@/lib/contextMenus";
import {
  branchIds, emptySources, newlyClosed, nodesForSource, sourceOf, toggle, visibleNodes,
  type Source,
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
  onToggle: (node: TreeNode) => void;
}) {
  // A history node is a session, or the year or month one is filed under —
  // one kind, told apart by whether anything sits beneath it.
  const Icon =
    node.kind === "folder" || node.kind === "directory" || (node.kind === "history" && branch)
      ? FolderIcon
      : node.kind === "history"
        ? HistoryIcon
        : node.kind === "allTracks"
          ? NoteIcon
          : node.kind === "device"
            ? DeviceIcon
            : ListIcon;
  // Whether the drag is over this row right now. Only the row under the
  // pointer is marked — every playlist lighting up for the whole drag read
  // as a grid of errors — and a drag that ends elsewhere clears it.
  const [over, setOver] = useState(false);
  useEffect(() => {
    if (!droppable) setOver(false);
  }, [droppable]);
  return (
    <div
      className={styles.node}
      data-selected={selected || undefined}
      data-kind={node.kind}
      style={{ paddingLeft: `${14 + node.depth * 20}px` }}
      // A note is information, not a place: nothing to select.
      onMouseDown={() => node.kind !== "note" && onSelect(node)}
      onDragOver={(e) => {
        // Only a playlist takes tracks: a folder holds playlists, and dropping
        // into one would have to invent which.
        if (!droppable) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
        setOver(true);
      }}
      onDragLeave={(e) => {
        // Leaving for one of the row's own children is not leaving the row.
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        setOver(false);
      }}
      onDrop={(e) => {
        setOver(false);
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
      data-over={(droppable && over) || undefined}
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
          onToggle(node);
        }}
        role={branch ? "button" : undefined}
        aria-label={branch ? `${open ? "Collapse" : "Expand"} ${node.name}` : undefined}
      />
      {node.kind === "collection" || node.kind === "histories" || node.kind === "explorer" ||
      node.kind === "note" ? null : (
        <Icon className={styles.icon} />
      )}
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
  /**
   * A lazy node was opened: read what is under it. The Explorer's folders,
   * whose children are not known until somebody looks.
   */
  onExpand?: (node: TreeNode) => void;
}

export function TreeView({
  nodes, selectedId, onSelect, dragging, onDropTracks, onExport,
  onCreatePlaylist, onCreateFolder, onDeleteNode, readOnly = false, onExpand,
}: TreeViewProps) {
  /** The tree menu: where it is, and which node it was opened on. */
  const [menu, setMenu] = useState<{ x: number; y: number; node: TreeNode } | null>(null);
  // Which nodes are closed. Seeded from the tree the backend sent — it marks
  // what should open, and 187 history sessions filed by year and month would
  // otherwise arrive on top of the playlists — and the user's own toggles take
  // over from there.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  // Every id seeded so far. Only a node the tree has never seen is seeded:
  // re-seeding on every later tree would shut whatever the user had opened
  // each time a playlist changed, and the Explorer's folders arrive a level
  // at a time, each level closed, long after the first tree.
  const seen = useRef<Set<string>>(new Set());
  useEffect(() => {
    const fresh = newlyClosed(nodes, seen.current);
    for (const node of nodes) seen.current.add(node.id);
    if (fresh.length === 0) return;
    setCollapsed((c) => {
      const next = new Set(c);
      for (const id of fresh) next.add(id);
      return next;
    });
  }, [nodes]);

  const onToggle = useCallback(
    (node: TreeNode) => {
      setCollapsed((c) => toggle(c, node.id));
      // Opening a lazy node is the moment to read what is under it. Told on
      // every open, and the owner ignores what it already holds.
      if (node.lazy === true && collapsed.has(node.id)) onExpand?.(node);
    },
    [collapsed, onExpand],
  );

  const empty = useMemo(() => emptySources(nodes), [nodes]);
  // Both derived in one pass each; per-row lookups would scan the array.
  const branches = useMemo(() => branchIds(nodes), [nodes]);
  const visible = useMemo(() => visibleNodes(nodes, collapsed), [nodes, collapsed]);

  // The rail is a shortcut, not a filter — `browseSetting.xml` calls it
  // TreeShortcut. rekordbox keeps one tree and jumps to a section; filtering
  // instead would hide All Tracks whenever Playlists was picked.
  const source = useMemo(() => sourceOf(nodes, selectedId), [nodes, selectedId]);
  // Set by a rail click, read once the selection has moved: the section's
  // row is scrolled to the top, which is what the capture shows — the
  // Explorer heading first in the pane with the playlists above it out of
  // sight. A click on a node scrolls nothing; it was in view to be clicked.
  const list = useRef<HTMLDivElement>(null);
  const jumped = useRef(false);
  const jumpTo = useCallback(
    (wanted: Source) => {
      const first = nodesForSource(nodes, wanted)[0];
      if (!first) return;
      jumped.current = true;
      onSelect(first);
    },
    [nodes, onSelect],
  );
  useEffect(() => {
    if (!jumped.current) return;
    jumped.current = false;
    const row = list.current?.querySelector<HTMLElement>("[data-selected]");
    row?.scrollIntoView({ block: "start" });
  }, [selectedId]);

  return (
    <nav className={styles.tree} aria-label="Library">
      <SourceRail selected={source} onSelect={jumpTo} empty={empty} />
      <div className={styles.nodes} role="tree" ref={list}>
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
        {/* Room to scroll the last section to the top of the pane, as the
            capture shows the Explorer with the playlists above it out of
            sight and nothing but panel below its last row [OBS]. */}
        <div className={styles.tail} aria-hidden />
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
