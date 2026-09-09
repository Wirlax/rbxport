/**
 * Export-mode shell.
 *
 * Composes the measured regions: top bar, library tree, track browser, status
 * bar. The preview player region is reserved but not yet implemented (it lands
 * with `rbl-audio` in Milestone 1).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { Backend, Device, LibrarySummary, RowDto, SortColumn, TreeNode, ViewSpec } from "@/ipc/types";
import { TrackTable } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { TopBar } from "@/views/topbar/TopBar";
import { StatusBar } from "@/views/statusbar/StatusBar";
import styles from "./App.module.css";
import { detectPlatform, dispatch } from "@/lib/shortcuts";
import { clampWidth, TREE_BOUNDS } from "@/lib/splitter";
import { exportSummary } from "@/lib/exportSummary";
import { deviceId, deviceNodes } from "@/lib/devices";
import { resolveMenu } from "@/lib/menu";
import { nextSort, specForNode, type SortState } from "@/lib/viewSpec";
import { loadSession, saveSession, SEEDED_NODES, SEEDED_ROWS } from "@/lib/session";
import { startWindowDrag, toggleWindowMaximise } from "@/lib/windowDrag";
import { deckCount, isFullDeck, isSideBySide, type PlayerLayout } from "@/lib/layout";
import { InfoPanel } from "@/views/info/InfoPanel";
import { SubBrowser } from "@/views/subbrowser/SubBrowser";
import { DevicePanel } from "@/views/devices/DevicePanel";
import { useColumns, type ColumnContext } from "@/store/useColumns";
import { Player } from "@/views/player/Player";
import { Settings } from "@/views/settings/Settings";
import { useAnalysis } from "@/store/useAnalysis";

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
  // Read once, synchronously, so the first render is already the layout the
  // window closed with rather than the default that then jumps.
  const [restored] = useState(loadSession);

  const [tree, setTree] = useState<readonly TreeNode[]>(restored.tree);
  // Connected volumes. Asked for, never polled: a 1 Hz scan of every mount
  // point is exactly the kind of idle work the budgets forbid.
  const [devices, setDevices] = useState<readonly Device[]>([]);
  const [syncing, setSyncing] = useState(false);
  // Closed by default, which is what browseSetting.xml records for the user's
  // own rekordbox (`ListInfo open="0"`).
  const [infoOpen, setInfoOpen] = useState(restored.infoOpen);
  // Closed unless it was open at exit; browseSetting.xml records
  // `SubBrowse open="0"` for a first run.
  const [subOpen, setSubOpen] = useState(restored.subOpen);
  // Why the library is not there, when it is not. Shown instead of "Loading…",
  // which is a lie once the load has failed.
  const [loadError, setLoadError] = useState<string | null>(null);
  const [summary, setSummary] = useState<LibrarySummary | null>(null);
  const [selectedNode, setSelectedNode] = useState<TreeNode | null>(null);
  // One piece of state, not two: updating `descending` from inside a `setSort`
  // updater made the toggle a side effect, and StrictMode's double invocation
  // cancelled it out.
  // Opens in the view's own order: a playlist's is the order somebody put it
  // in, and the collection has no more meaningful default.
  const [sortState, setSortState] = useState<SortState>(restored.sort);
  const [selectedCount, setSelectedCount] = useState(0);
  // The rows behind the selection, so they can be queued for analysis.
  const [selectedTracks, setSelectedTracks] = useState<{ id: string; title: string }[]>([]);
  // The row the player is showing. Set by the browser as the selection moves,
  // so the player reflects what is highlighted rather than nothing.
  const [playerTrack, setPlayerTrack] = useState<RowDto | null>(restored.player);
  const [query, setQuery] = useState("");
  // The tree's width, dragged by the splitter. Held here because the grid that
  // sizes both panes lives here.
  const [treeWidth, setTreeWidth] = useState(restored.treeWidth);
  const bodyRef = useRef<HTMLDivElement>(null);
  const dragFrom = useRef<{ x: number; width: number } | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const clock = useClock();
  // The table's layout follows the kind of thing being browsed, as
  // browseSetting.xml does, rather than each individual playlist.
  const columnContext: ColumnContext =
    selectedNode?.kind === "playlist"
      ? "playlist"
      : selectedNode?.kind === "history"
        ? "history"
        : "collection";
  const cols = useColumns(columnContext);
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** How much of the window the deck takes, kept across restarts. */
  const [layout, setLayout] = useState<PlayerLayout>(restored.layout);
  // An analysed track's waveform and key change, so its row is stale.
  const analysis = useAnalysis(
    useCallback((id: string) => {
      setPendingEdits((edits) => new Map(edits).set(id, { analysed: 1 }));
    }, []),
  );
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
    let stopReady: (() => void) | undefined;
    let stopError: (() => void) | undefined;

    /** One attempt at the first load. False means the library is not up yet. */
    const attempt = async (backend: Backend) => {
      try {
        // Devices are deliberately not in here. A stick that cannot be read
        // must not stop the collection from appearing, and it used to: this
        // was one `Promise.all`, so any of the three failing left the window
        // on "Loading…" with nothing said.
        const [nodes, info] = await Promise.all([
          backend.playlistTree(),
          backend.librarySummary(),
        ]);
        if (cancelled) return true;
        setTree(nodes);
        setSummary(info);
        setLoadError(null);
        // The playlist that was open at exit, when it is still there — it can
        // have been deleted between runs, so this is a lookup, not a promise.
        const remembered = nodes.find((n) => n.id === restored.selectedNodeId);
        setSelectedNode(remembered ?? nodes.find((n) => n.kind === "playlist") ?? nodes[0] ?? null);
        void backend
          .listDevices()
          .then((volumes) => {
            if (!cancelled) setDevices(volumes);
          })
          .catch(() => {
            // Nothing to say: no devices is the normal case.
          });
        return true;
      } catch {
        // The usual reason is that the backend is still reading the library,
        // which the ready event below will tell us about.
        return false;
      }
    };

    void (async () => {
      const backend = await getBackend();
      if (cancelled) return;
      // Subscribed before the first attempt, not after. The library can become
      // ready in the gap between a failed attempt and a later subscription,
      // and that gap is exactly where the window used to get stuck.
      stopReady = backend.onLibraryReady(() => {
        void attempt(backend);
      });
      stopError = backend.onLibraryError((message) => {
        if (!cancelled) setLoadError(message);
      });
      await attempt(backend);
    })();

    return () => {
      cancelled = true;
      stopReady?.();
      stopError?.();
    };
    // `restored` is read once and never changes, but the rule cannot know that
    // and the id is genuinely read here.
  }, [restored.selectedNodeId]);

  const spec: ViewSpec = useMemo(
    () => specForNode(selectedNode, query, sortState),
    [selectedNode, sortState, query],
  );

  const handleSort = useCallback((column: SortColumn) => {
    setSortState((s) => nextSort(s, column));
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

  /** Queues whatever is selected in the browser. */
  const analyseSelection = useCallback(() => {
    if (selectedTracks.length === 0) return;
    analysis.add(selectedTracks);
  }, [analysis, selectedTracks]);

  const importFromMenu = useCallback(async () => {
    setDropNote("Choosing files to import…");
    try {
      const backend = await getBackend();
      const report = await backend.importFiles();
      if (report === null) {
        setDropNote(null);
        return;
      }
      const total = report.imported + report.skipped.length;
      setDropNote(
        report.skipped.length === 0
          ? `Imported ${report.imported} of ${total} files.`
          : `Imported ${report.imported} of ${total} files; ${report.skipped.length} skipped.`,
      );
      setTree(await backend.playlistTree());
    } catch (e) {
      setDropNote(e instanceof Error ? e.message : "Those files could not be imported.");
    }
  }, []);

  // Native menu clicks. The shell sends the item's id and nothing else; what
  // it means, and whether it is allowed right now, is decided in one place.
  useEffect(() => {
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      stop = backend.onMenu((id) => {
        const outcome = resolveMenu(id, summary?.readOnly ?? false);
        if (!outcome) return;
        if ("refused" in outcome) {
          setDropNote(outcome.refused);
          return;
        }
        if (outcome.action === "import") {
          void importFromMenu();
          return;
        }
        if (outcome.action === "info") {
          setInfoOpen((open) => !open);
          return;
        }
        if (outcome.action === "sub") {
          setSubOpen((open) => !open);
          return;
        }
        // Both the settings panel and the missing-file manager live in
        // Settings, so either opens it.
        setSettingsOpen(true);
      });
    })();
    return () => stop?.();
  }, [summary?.readOnly, importFromMenu]);

  const refreshDevices = useCallback(() => {
    void (async () => {
      const backend = await getBackend();
      setDevices(await backend.listDevices());
    })();
  }, []);

  // Devices join the tree as nodes so the Devices section renders through the
  // same path as every other section.
  const treeNodes = useMemo(() => [...tree, ...deviceNodes(devices)], [tree, devices]);
  const selectedDevice = useMemo(
    () => devices.find((device) => deviceId(device) === selectedNode?.id) ?? null,
    [devices, selectedNode],
  );

  const syncToDevice = useCallback(
    async (playlistId: string) => {
      if (!selectedDevice) return;
      const name = tree.find((n) => n.id === playlistId)?.name ?? "the playlist";
      setSyncing(true);
      setDropNote(`Writing ${name} to ${selectedDevice.name}…`);
      try {
        const backend = await getBackend();
        const report = await backend.exportPlaylist(playlistId, selectedDevice.path);
        setDropNote(report === null ? null : exportSummary(selectedDevice.name, report));
        setDevices(await backend.listDevices());
      } catch (e) {
        setDropNote(e instanceof Error ? e.message : "That export could not be written.");
      } finally {
        setSyncing(false);
      }
    },
    [selectedDevice, tree],
  );

  const exportPlaylist = useCallback((node: TreeNode) => {
    void (async () => {
      const backend = await getBackend();
      setDropNote(`Exporting ${node.name}…`);
      try {
        const report = await backend.exportPlaylist(node.id);
        if (report === null) {
          setDropNote(null);
          return;
        }
        setDropNote(exportSummary(node.name, report));
      } catch (e) {
        setDropNote(e instanceof Error ? e.message : "That export could not be written.");
      }
    })();
  }, []);

  // The top of the current view, kept only to write the next start's opening
  // screen. The library itself still lives entirely in Rust.
  const [screen, setScreen] = useState<{ rows: RowDto[]; count: number }>({
    rows: restored.rows,
    count: restored.count,
  });
  const onFirstRows = useCallback((rows: RowDto[], count: number) => {
    setScreen({ rows, count });
  }, []);

  // Written on every change rather than on exit: a window that is force-quit,
  // or a machine that loses power, still comes back where it was. It is a few
  // hundred bytes to localStorage, not something worth batching.
  useEffect(() => {
    saveSession({
      treeWidth,
      selectedNodeId: selectedNode?.id ?? null,
      sort: sortState,
      infoOpen,
      subOpen,
      // Only once the real library is up: storing the seed back over itself
      // would keep the first run's rows alive forever.
      tree: [...tree].slice(0, SEEDED_NODES),
      rows: screen.rows.slice(0, SEEDED_ROWS),
      count: screen.count,
      player: playerTrack,
      layout,
    });
  }, [treeWidth, selectedNode, sortState, infoOpen, subOpen, tree, screen, playerTrack, layout]);

  // The last screen, handed to the table until the backend answers. Dropped as
  // soon as the library is up, so a stale row cannot outlive its replacement.
  const seed = useMemo(
    () =>
      summary === null && restored.rows.length > 0
        ? { count: restored.count, rows: restored.rows }
        : undefined,
    [summary, restored.count, restored.rows],
  );

  const selectionText =
    selectedCount > 1 ? `Selected: ${selectedCount} Tracks` : selectedCount === 1 ? "Selected: 1 Track" : "";

  return (
    <div className={styles.window}>
      <div
        className={styles.titleBar}
        data-testid="title-bar"
        onMouseDown={startWindowDrag}
        onDoubleClick={toggleWindowMaximise}
      >
        rekordbox-lite
      </div>
      <TopBar
        clock={clock}
        onOpenSettings={() => setSettingsOpen(true)}
        layout={layout}
        onLayoutChange={setLayout}
      />
      {/* Full Browser draws no deck at all, and no gutter under one. */}
      {deckCount(layout) > 0 ? (
        <div className={styles.decks} data-side-by-side={isSideBySide(layout) ? "" : undefined}>
          <Player
            track={playerTrack}
            onEject={() => setPlayerTrack(null)}
            simple={!isFullDeck(layout)}
          />
          {deckCount(layout) > 1 ? <Player deck="b" track={null} /> : null}
          <div className={styles.playerGutter} aria-hidden />
        </div>
      ) : null}
      <div
        data-testid="body"
        className={styles.body}
        ref={bodyRef}
        style={{ ["--tree-w" as string]: `${treeWidth}px` }}
        data-info={infoOpen ? "" : undefined}
        data-sub={subOpen ? "" : undefined}
      >
        <TreeView
          nodes={treeNodes}
          selectedId={selectedNode?.id ?? null}
          onSelect={setSelectedNode}
          dragging={draggedTracks !== null}
          onDropTracks={addDraggedTo}
          onExport={exportPlaylist}
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
        {selectedDevice ? (
          <DevicePanel
            device={selectedDevice}
            playlists={tree}
            onSync={syncToDevice}
            onRefresh={refreshDevices}
            busy={syncing}
          />
        ) : (
        <TrackTable
          spec={spec}
          onSortChange={handleSort}
          onSelectionChange={setSelectedCount}
          onSelectedTracks={setSelectedTracks}
          onAnalyse={analyseSelection}
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
          seed={seed}
          onFirstRows={onFirstRows}
        />
        )}
        {subOpen ? (
          <SubBrowser
            nodes={tree}
            libraryGeneration={libraryGeneration}
            onClose={() => setSubOpen(false)}
          />
        ) : null}
        {infoOpen ? <InfoPanel track={playerTrack} onClose={() => setInfoOpen(false)} /> : null}
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
        activity={
          analysis.running
            ? `Analyzing: ${analysis.state.done + analysis.state.failed.length + 1} of ${analysis.total}` +
              (analysis.state.current ? ` — ${analysis.state.current.title}` : "")
            : (dropNote ??
              loadError ??
              (summary ? `${summary.trackCount} Tracks` : "Loading the library…"))
        }
        onCancelAnalysis={analysis.running ? analysis.cancel : undefined}
        analysisFailures={analysis.state.failed.length}
        selection={selectionText}
        readOnly={summary?.readOnly ?? false}
      />
    </div>
  );
}
