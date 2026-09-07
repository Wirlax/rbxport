/**
 * Export-mode shell.
 *
 * Composes the measured regions: top bar, library tree, track browser, status
 * bar. The preview player region is reserved but not yet implemented (it lands
 * with `rbl-audio` in Milestone 1).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { LibrarySummary, SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { TrackTable } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { TopBar } from "@/views/topbar/TopBar";
import { StatusBar } from "@/views/statusbar/StatusBar";
import styles from "./App.module.css";

function useClock(): string {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    // Tick on the minute boundary rather than polling: idle CPU budget is 0.5%.
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      timer = setTimeout(() => {
        setNow(new Date());
        schedule();
      }, 60_000 - (Date.now() % 60_000));
    };
    schedule();
    return () => clearTimeout(timer);
  }, []);
  return `${now.getHours() % 12 || 12}:${String(now.getMinutes()).padStart(2, "0")}`;
}

export function App() {
  const [tree, setTree] = useState<readonly TreeNode[]>([]);
  const [summary, setSummary] = useState<LibrarySummary | null>(null);
  const [selectedNode, setSelectedNode] = useState<TreeNode | null>(null);
  // One piece of state, not two: updating `descending` from inside a `setSort`
  // updater made the toggle a side effect, and StrictMode's double invocation
  // cancelled it out.
  const [sortState, setSortState] = useState<{ column: SortColumn; descending: boolean }>({
    column: "trackNo",
    descending: false,
  });
  const [selectedCount, setSelectedCount] = useState(0);
  const clock = useClock();

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const backend = await getBackend();
      const [nodes, info] = await Promise.all([backend.playlistTree(), backend.librarySummary()]);
      if (cancelled) return;
      setTree(nodes);
      setSummary(info);
      setSelectedNode(nodes.find((n) => n.kind === "playlist") ?? nodes[0] ?? null);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const spec: ViewSpec = useMemo(
    () => ({
      source:
        selectedNode?.kind === "playlist"
          ? { kind: "playlist", id: selectedNode.id }
          : { kind: "collection" },
      sort: sortState.column,
      descending: sortState.descending,
      query: "",
    }),
    [selectedNode, sortState],
  );

  const handleSort = useCallback((column: SortColumn) => {
    setSortState((s) =>
      s.column === column ? { column, descending: !s.descending } : { column, descending: false },
    );
  }, []);

  const selectionText =
    selectedCount > 1 ? `Selected: ${selectedCount} Tracks` : selectedCount === 1 ? "Selected: 1 Track" : "";

  return (
    <div className={styles.window}>
      <TopBar clock={clock} />
      <section className={styles.player} aria-label="Preview player" />
      <div className={styles.body}>
        <TreeView nodes={tree} selectedId={selectedNode?.id ?? null} onSelect={setSelectedNode} />
        <TrackTable
          spec={spec}
          onSortChange={handleSort}
          onSelectionChange={setSelectedCount}
          title={selectedNode?.name ?? "Collection"}
        />
      </div>
      <StatusBar
        activity={summary ? `${summary.trackCount} Tracks` : "Loading…"}
        selection={selectionText}
        readOnly={summary?.readOnly ?? false}
      />
    </div>
  );
}
