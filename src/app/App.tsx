/**
 * Export-mode shell.
 *
 * Composes the measured regions: top bar, library tree, track browser, status
 * bar. The preview player region is reserved but not yet implemented (it lands
 * with `rbl-audio` in Milestone 1).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { LibrarySummary, RowDto, SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { TrackTable } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { TopBar } from "@/views/topbar/TopBar";
import { StatusBar } from "@/views/statusbar/StatusBar";
import styles from "./App.module.css";
import { detectPlatform, dispatch } from "@/lib/shortcuts";
import { clampWidth, TREE_BOUNDS } from "@/lib/splitter";
import { useColumns } from "@/store/useColumns";
import { Player } from "@/views/player/Player";
import { Settings } from "@/views/settings/Settings";

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
  // The row the player is showing. Set by the browser as the selection moves,
  // so the player reflects what is highlighted rather than nothing.
  const [playerTrack, setPlayerTrack] = useState<RowDto | null>(null);
  const [query, setQuery] = useState("");
  // The tree's width, dragged by the splitter. Held here because the grid that
  // sizes both panes lives here.
  const [treeWidth, setTreeWidth] = useState(305);
  const bodyRef = useRef<HTMLDivElement>(null);
  const dragFrom = useRef<{ x: number; width: number } | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const clock = useClock();
  const cols = useColumns();
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Track ids in flight from the browser to the tree.
  const [draggedTracks, setDraggedTracks] = useState<readonly string[] | null>(null);
  const [dropNote, setDropNote] = useState<string | null>(null);
  // Bumped whenever the library changes underneath us, which drops cached
  // pages. Without it an edit's effect never reached the table.
  const [libraryGeneration, setLibraryGeneration] = useState(0);
  // Edits shown at once, dropped when the backend's reload lands. A write
  // makes the backend re-read the library — 243 ms on the real collection —
  // and waiting for that before a star fills in feels broken.
  const [pendingEdits, setPendingEdits] = useState<Map<string, Partial<RowDto>>>(
    () => new Map(),
  );
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

  useEffect(() => {
    let stop: (() => void) | undefined;
    let live = true;
    void (async () => {
      const backend = await getBackend();
      if (!live) return;
      stop = backend.onLibraryChanged((generation) => {
        setLibraryGeneration(generation);
        // The reload carries the edits, so the overlay has done its job.
        setPendingEdits(new Map());
        // The tree can change too — a playlist gained tracks, or one was
        // deleted — so it is re-read rather than assumed still right.
        void backend.playlistTree().then(setTree);
      });
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const bounds = useCallback(
    () => ({ ...TREE_BOUNDS, available: bodyRef.current?.clientWidth ?? 0 }),
    [],
  );

  // Re-clamp when the window changes: a width that fitted a wide window can
  // leave the track list with nothing in a narrow one.
  useEffect(() => {
    const body = bodyRef.current;
    if (!body || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      setTreeWidth((w) => clampWidth(w, bounds()));
    });
    observer.observe(body);
    return () => {
      observer.disconnect();
    };
  }, [bounds]);

  const onSplitterDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      dragFrom.current = { x: event.clientX, width: treeWidth };
      // Capture, so the drag survives the pointer leaving the 4px handle.
      event.currentTarget.setPointerCapture(event.pointerId);
      event.preventDefault();
    },
    [treeWidth],
  );

  const onSplitterMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const from = dragFrom.current;
      if (!from) return;
      setTreeWidth(clampWidth(from.width + (event.clientX - from.x), bounds()));
    },
    [bounds],
  );

  const onSplitterUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    dragFrom.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
  }, []);

  /** Runs one edit and reports what happened, refusals included. */
  const runEdit = useCallback(
    async (what: string, edit: (b: Awaited<ReturnType<typeof getBackend>>) => Promise<unknown>) => {
      const backend = await getBackend();
      try {
        await edit(backend);
        setDropNote(what);
      } catch (e) {
        setDropNote(e instanceof Error ? e.message : "That could not be saved.");
      }
    },
    [],
  );

  /** Shows an edit at once, so the interface does not wait on the reload. */
  const showPending = useCallback((id: string, patch: Partial<RowDto>) => {
    setPendingEdits((edits) => new Map(edits).set(id, { ...edits.get(id), ...patch }));
  }, []);

  const rateTrack = useCallback(
    (id: string, stars: number) => {
      showPending(id, { rating: stars });
      void runEdit(stars === 0 ? "Rating cleared." : `Rated ${stars} of 5.`, (b) =>
        b.edits.setTrackRating(id, stars),
      );
    },
    [runEdit, showPending],
  );

  const commentTrack = useCallback(
    (id: string, comment: string) => {
      showPending(id, { comment });
      void runEdit("Comment saved.", (b) => b.edits.setTrackComment(id, comment));
    },
    [runEdit, showPending],
  );

  const addDraggedTo = useCallback(
    (playlistId: string) => {
      const ids = draggedTracks;
      setDraggedTracks(null);
      if (!ids || ids.length === 0) return;
      void (async () => {
        const backend = await getBackend();
        try {
          await backend.edits.addTracksToPlaylist(playlistId, [...ids]);
          const name = tree.find((n) => n.id === playlistId)?.name ?? "the playlist";
          setDropNote(`Added ${ids.length} track${ids.length === 1 ? "" : "s"} to ${name}.`);
        } catch (e) {
          // The refusal that matters is Rekordbox holding the database; say so
          // rather than letting the drop look as if it worked.
          setDropNote(e instanceof Error ? e.message : "That could not be saved.");
        }
      })();
    },
    [draggedTracks, tree],
  );

  // Clear the note after a moment: it reports an action, not a state.
  useEffect(() => {
    if (dropNote === null) return;
    const timer = setTimeout(() => setDropNote(null), 4000);
    return () => {
      clearTimeout(timer);
    };
  }, [dropNote]);

  const selectionText =
    selectedCount > 1 ? `Selected: ${selectedCount} Tracks` : selectedCount === 1 ? "Selected: 1 Track" : "";

  return (
    <div className={styles.window}>
      <TopBar clock={clock} onOpenSettings={() => setSettingsOpen(true)} />
      <Player track={playerTrack} />
      <div
        className={styles.body}
        ref={bodyRef}
        style={{ ["--tree-w" as string]: `${treeWidth}px` }}
      >
        <TreeView
          nodes={tree}
          selectedId={selectedNode?.id ?? null}
          onSelect={setSelectedNode}
          dragging={draggedTracks !== null}
          onDropTracks={addDraggedTo}
        />
        <div
          className={styles.splitter}
          onPointerDown={onSplitterDown}
          onPointerMove={onSplitterMove}
          onPointerUp={onSplitterUp}
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize the library tree"
          aria-valuenow={treeWidth}
        />
        <TrackTable
          spec={spec}
          onSortChange={handleSort}
          onSelectionChange={setSelectedCount}
          onFocusedRow={setPlayerTrack}
          onDragTracks={setDraggedTracks}
          onRate={rateTrack}
          onComment={commentTrack}
          libraryGeneration={libraryGeneration}
          pendingEdits={pendingEdits}
          title={selectedNode?.name ?? "Collection"}
          query={query}
          onQueryChange={setQuery}
          searchRef={searchRef}
          columns={cols.columns}
          onColumnMove={cols.move}
          onColumnResize={cols.resize}
          onColumnToggle={cols.toggle}
          onColumnAutoSize={cols.autoSize}
          onColumnAutoSizeAll={cols.autoSizeEvery}
        />
      </div>
      {settingsOpen ? (
        <Settings
          summary={summary}
          onResetColumns={cols.reset}
          onResetLayout={() => setTreeWidth(clampWidth(305, bounds()))}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}

      <StatusBar
        activity={dropNote ?? (summary ? `${summary.trackCount} Tracks` : "Loading…")}
        selection={selectionText}
        readOnly={summary?.readOnly ?? false}
      />
    </div>
  );
}
