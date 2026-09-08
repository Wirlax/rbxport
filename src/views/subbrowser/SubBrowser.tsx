/**
 * The sub-browser: a second tree and track list, with its own selection.
 *
 * Not a variant of the main browser — a genuinely independent one, which is
 * how rekordbox stores it. `browseSetting.xml` keeps a whole second
 * `SubTreeLayout` with its own expand state and its own
 * `SubTreeLayoutSelectedPlaylistID`, distinct from the main
 * `TreeLayoutSelectedPlaylistID`. That is what it is for: comparing two
 * playlists, or dragging between them, without losing your place in either.
 *
 * Its width is the one that file records (`SubBrowse w=1107`), and like the
 * information panel it starts closed, which that file records too.
 *
 * **The arrangement is ours, not measured.** No capture of rekordbox's own
 * sub-browser was ever taken. What is not guesswork: that it is a second tree
 * and list pair, that it holds its own selection, its width, and that it
 * starts closed — all four are in that file. Recorded in TODO.md.
 */
import { useCallback, useMemo, useState } from "react";

import type { SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { TrackTable } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { useColumns } from "@/store/useColumns";
import { nextSort, specForNode, type SortState, DEFAULT_SORT } from "@/lib/viewSpec";
import styles from "./SubBrowser.module.css";

export interface SubBrowserProps {
  nodes: readonly TreeNode[];
  /** Bumped when the library changes, so cached pages are dropped. */
  libraryGeneration: number;
  onClose: () => void;
}

export function SubBrowser({ nodes, libraryGeneration, onClose }: SubBrowserProps) {
  // Its own selection, deliberately not shared with the main browser.
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortState | null>(null);
  // Its own columns too: a sub-browser is usually kept narrow, and forcing it
  // to share the main table's widths would make it useless.
  const cols = useColumns("subBrowser");

  const spec: ViewSpec = useMemo(
    () => specForNode(selected, query, sort),
    [selected, query, sort],
  );

  const onSortChange = useCallback((column: SortColumn) => {
    setSort((current) => nextSort(current ?? DEFAULT_SORT, column));
  }, []);

  return (
    <section className={styles.sub} aria-label="Sub-Browser">
      <header className={styles.head}>
        <span className={styles.title}>Sub-Browser</span>
        <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
          ×
        </button>
      </header>
      <div className={styles.body}>
        <TreeView
          nodes={nodes}
          selectedId={selected?.id ?? null}
          onSelect={setSelected}
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
        />
      </div>
    </section>
  );
}
