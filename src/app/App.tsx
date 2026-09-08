/**
 * Export-mode shell.
 *
 * Composes the measured regions: top bar, library tree, track browser, status
 * bar. The preview player region is reserved but not yet implemented (it lands
 * with `rbl-audio` in Milestone 1).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { LibrarySummary, SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { TrackTable } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { TopBar } from "@/views/topbar/TopBar";
import { StatusBar } from "@/views/statusbar/StatusBar";
import styles from "./App.module.css";
import { detectPlatform, dispatch } from "@/lib/shortcuts";

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
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement | null>(null);
  const clock = useClock();
  // Read once: the platform cannot change while the window is open, and
  // deciding it per key press would run a regex on every stroke.
  const platform = useMemo(detectPlatform, []);

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
      query,
    }),
    [selectedNode, sortState, query],
  );

  const handleSort = useCallback((column: SortColumn) => {
    setSortState((s) =>
      s.column === column ? { column, descending: !s.descending } : { column, descending: false },
    );
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const action = dispatch(event, platform, event.target as HTMLElement | null);
      if (action === null) return;
      // Only the actions handled here are swallowed; everything else falls
      // through to the browser and the OS.
      switch (action) {
        case "focusSearch":
          event.preventDefault();
          searchRef.current?.focus();
          searchRef.current?.select();
          break;
        case "clearSearch":
          // Escape in an empty box should blur rather than do nothing, so the
          // next arrow key reaches the track list.
          if (query === "") {
            searchRef.current?.blur();
          } else {
            setQuery("");
          }
          break;
        default:
          // Movement and selection live in the table; it listens for itself.
          return;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [platform, query]);

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
          query={query}
          onQueryChange={setQuery}
          searchRef={searchRef}
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
