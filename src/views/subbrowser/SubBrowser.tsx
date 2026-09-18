/**
 * The sub-browser: a second, complete browser beside the main one.
 *
 * Not a variant of the main browser — a genuinely independent one, which is
 * how rekordbox stores it. `browseSetting.xml` keeps a whole second
 * `SubTreeLayout` with its own expand state and its own
 * `SubTreeLayoutSelectedPlaylistID`, distinct from the main
 * `TreeLayoutSelectedPlaylistID`. That is what it is for: comparing two
 * playlists, or dragging between them, without losing your place in either.
 *
 * The arrangement is the 2026-09-09 capture's (`docs/screenshots`,
 * `9.09.26 PM`) [OBS]: the main browser narrowed, and beside it a second
 * source rail, a second tree, and a second track list with its own header,
 * search and count, all at the browser's full height. A splitter stands
 * between the main list and the sub-browser's rail, and another between the
 * sub-browser's tree and its list, both clamped the way the main tree's is.
 * The widths are the capture's: 878pt for the whole panel and 298pt for its
 * rail and tree, in a 1798pt window.
 *
 * The capture also shows a Tree View / Column View toggle over both trees,
 * which this app leaves out on purpose — TODO.md records the column view's
 * removal as a choice — so neither tree draws one.
 *
 * The list is a second view in Rust (`specForNode` gives it its own
 * `ViewSpec`, so `useTrackView` opens its own `view_id`), never a copy of
 * the main list's rows; the two share nothing but the library.
 */
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { usePreferences } from "@/store/usePreferences";
import { TrackTable, type TrackTableProps } from "@/views/browser/TrackTable";
import { TreeView, type TreeViewProps } from "@/views/tree/TreeView";
import { useColumns } from "@/store/useColumns";
import { clampWidth, subBounds, subTreeBounds } from "@/lib/splitter";
import { nextSort, specForNode, type SortState, DEFAULT_SORT } from "@/lib/viewSpec";
import styles from "./SubBrowser.module.css";

/** What the sub-browser's tree does that the shell has to do for it. */
export type SubTreeProps = Pick<
  TreeViewProps,
  "dragging" | "onDropTracks" | "onExport" | "onExportFile" | "onCreatePlaylist" | "onCreateFolder"
  | "onDeleteNode" | "onRenameNode" | "readOnly"
>;

/** Likewise for its list: dragging out, loading decks, the writes. */
export type SubListProps = Pick<
  TrackTableProps,
  "onDragTracks" | "players" | "onLoadTrack" | "onShowInFinder" | "onRate" | "onComment"
  | "pendingEdits" | "readOnly"
>;

export interface SubBrowserProps {
  nodes: readonly TreeNode[];
  /** Bumped when the library changes, so cached pages are dropped. */
  libraryGeneration: number;
  /**
   * Its width and its tree's, as last dragged, before clamping. The shell
   * keeps both across runs; the panel clamps them to the window it is in.
   */
  width: number;
  treeWidth: number;
  onWidthChange: (width: number) => void;
  onTreeWidthChange: (width: number) => void;
  tree: SubTreeProps;
  list: SubListProps;
}

/**
 * The width the panel shares with the main list: its parent's, less every
 * other pane in the row — the main tree and its gutter, the information
 * panel when it is open, and the icon column always. The main list is the
 * one immediately before it.
 *
 * Read from the panel's own element rather than a ref handed down from the
 * shell: a child's layout effect runs before the shell's ref is attached, so
 * on a run that opens with the panel already showing that ref is still empty
 * at the moment it is read. Measured after every render, since the panels
 * beside it come and go with the shell's renders and none of them resizes
 * this one; setting the same number again is free.
 */
function useSharedWidth(ref: React.RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);
  const measure = useCallback(() => {
    const el = ref.current;
    const parent = el?.parentElement;
    if (!el || !parent) return;
    const shared = el.previousElementSibling;
    let taken = 0;
    for (const sibling of parent.children) {
      if (sibling === el || sibling === shared) continue;
      taken += sibling.getBoundingClientRect().width;
    }
    setWidth(Math.round(parent.clientWidth - taken));
  }, [ref]);
  // Before the first paint, so the panel opens at its clamped width rather
  // than at the floor and then jumping.
  useLayoutEffect(measure);
  useLayoutEffect(() => {
    const parent = ref.current?.parentElement;
    if (!parent || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(parent);
    return () => {
      observer.disconnect();
    };
  }, [ref, measure]);
  return width;
}

/**
 * A pointer drag on a splitter, reported as the width it asks for.
 *
 * `sign` is which way the pane grows: +1 when the handle is on the pane's
 * right edge, -1 when it is on the left, as the sub-browser's outer one is.
 */
function useSplitterDrag(width: number, sign: 1 | -1, onChange: (width: number) => void) {
  const from = useRef<{ x: number; width: number } | null>(null);
  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      from.current = { x: event.clientX, width };
      // Capture, so the drag survives the pointer leaving the handle.
      event.currentTarget.setPointerCapture(event.pointerId);
      event.preventDefault();
    },
    [width],
  );
  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const start = from.current;
      if (!start) return;
      onChange(start.width + sign * (event.clientX - start.x));
    },
    [onChange, sign],
  );
  const onPointerUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    from.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
  }, []);
  return { onPointerDown, onPointerMove, onPointerUp };
}

export function SubBrowser({
  nodes, libraryGeneration, width, treeWidth, onWidthChange, onTreeWidthChange, tree, list,
}: SubBrowserProps) {
  const self = useRef<HTMLElement>(null);
  // Its own selection, deliberately not shared with the main browser. Held as
  // an id and looked up, so a playlist deleted from either tree drops out of
  // the selection rather than lingering as a node the tree no longer has.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = useMemo(
    () => nodes.find((node) => node.id === selectedId) ?? null,
    [nodes, selectedId],
  );
  const onSelect = useCallback((node: TreeNode) => setSelectedId(node.id), []);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortState | null>(null);
  // Its own columns too: a sub-browser is usually kept narrow, and forcing it
  // to share the main table's widths would make it useless.
  const cols = useColumns("subBrowser");

  const keyDisplay = usePreferences().view.keyDisplay;
  const spec: ViewSpec = useMemo(
    () => specForNode(selected, query, sort, keyDisplay),
    [selected, query, sort, keyDisplay],
  );

  const onSortChange = useCallback((column: SortColumn) => {
    setSort((current) => nextSort(current ?? DEFAULT_SORT, column));
  }, []);

  // The stored widths are what was asked for; what is drawn is what the
  // window allows. Deriving it here rather than writing it back means a
  // window that shrinks and grows again gives the panel its width back.
  const available = useSharedWidth(self);
  const shown = clampWidth(width, subBounds(available));
  const treeShown = clampWidth(treeWidth, subTreeBounds(shown));

  const outer = useSplitterDrag(shown, -1, onWidthChange);
  const inner = useSplitterDrag(treeShown, 1, onTreeWidthChange);

  return (
    <section
      ref={self}
      className={styles.sub}
      aria-label="Sub-Browser"
      style={{
        width: `${shown}px`,
        ["--sub-tree-w" as string]: `${treeShown}px`,
      }}
    >
      {/* On the panel's left edge, between the main list and this rail. The
          capture draws no gap there at all, so the handle has no width of its
          own and is caught through the few points either side of the edge. */}
      <div
        className={styles.edge}
        {...outer}
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the sub-browser"
        aria-valuenow={shown}
      />
      <TreeView
        nodes={nodes}
        selectedId={selectedId}
        onSelect={onSelect}
        {...tree}
      />
      <div
        className={styles.splitter}
        {...inner}
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the sub-browser's tree"
        aria-valuenow={treeShown}
      />
      <TrackTable
        spec={spec}
        onSortChange={onSortChange}
        title={selected?.name ?? "Collection"}
        query={query}
        onQueryChange={setQuery}
        columns={cols.columns}
        onColumnMove={cols.move}
        onColumnResize={cols.resize}
        onColumnToggle={cols.toggle}
        onColumnAutoSize={cols.autoSize}
        onColumnAutoSizeAll={cols.autoSizeEvery}
        libraryGeneration={libraryGeneration}
        {...list}
      />
    </section>
  );
}
