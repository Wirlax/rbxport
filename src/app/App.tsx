/**
 * Export-mode shell.
 *
 * Composes the measured regions: top bar, library tree, track browser, status
 * bar. The preview player region is reserved but not yet implemented (it lands
 * with `rbl-audio` in Milestone 1).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type {
  Backend, DeckId, Device, LibrarySummary, RowDto, SortColumn, TreeNode, ViewSpec,
} from "@/ipc/types";
import { TrackTable, type TrackDrag } from "@/views/browser/TrackTable";
import { TreeView } from "@/views/tree/TreeView";
import { TopBar } from "@/views/topbar/TopBar";
import { StatusBar } from "@/views/statusbar/StatusBar";
import styles from "./App.module.css";
import { detectPlatform, dispatch, menuAccelerator } from "@/lib/shortcuts";
import { clampWidth, TREE_BOUNDS } from "@/lib/splitter";
import { exportSummary } from "@/lib/exportSummary";
import { deviceId, deviceNodes } from "@/lib/devices";
import { refusal, resolveMenu } from "@/lib/menu";
import { nextSort, specForNode, type SortState } from "@/lib/viewSpec";
import {
  DEFAULT_SUB_TREE_WIDTH, DEFAULT_SUB_WIDTH, loadSession, saveSession, SEEDED_NODES, SEEDED_ROWS,
  type TrafficLightSource,
} from "@/lib/session";
import { startWindowDrag, toggleWindowMaximise } from "@/lib/windowDrag";
import { AppCost } from "@/views/topbar/AppCost";
import { useLimiter } from "@/store/useLimiter";
import { useUpdater } from "@/store/useUpdater";
import { UpdateManager } from "@/views/update/UpdateManager";
import { useMaster } from "@/store/useMaster";
import { asLayout, deckCount, isFullDeck, type PlayerLayout } from "@/lib/layout";
import { InfoPanel } from "@/views/info/InfoPanel";
import { SubBrowser } from "@/views/subbrowser/SubBrowser";
import { RightRail } from "@/views/browser/RightRail";
import { DevicePanel } from "@/views/devices/DevicePanel";
import { useColumns, type ColumnContext } from "@/store/useColumns";
import { useExplorer } from "@/store/useExplorer";
import { isLooseId } from "@/lib/explorer";
import { parentFor } from "@/lib/tree";
import { DETAIL_BARS, JUMP_SIZE_ID } from "@/lib/player";
import type { Deck as SyncDeck } from "@/lib/sync";
import { LayoutDualIcon } from "@/components/icons";
import { Player } from "@/views/player/Player";
import { MixerStrip } from "@/views/player/MixerStrip";
import { DualZoom } from "@/views/player/DualDeck";
import { Preferences, type Pane } from "@/views/settings/Preferences";
import { PreferencesProvider, usePreferencesStore } from "@/store/usePreferences";
import { useAnalysis } from "@/store/useAnalysis";
import { TrackFilter } from "@/views/browser/TrackFilter";
import { EMPTY_FILTER, toSpecFilter, type FilterState } from "@/lib/trackFilter";
import type { FilterValues } from "@/ipc/types";
import { useTooltip } from "@/store/usePreferences";

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
  // The sub-browser's width and its tree's, as last dragged. Clamped to the
  // window by the panel itself, so these are what was asked for.
  const [subWidth, setSubWidth] = useState(restored.subWidth);
  const [subTreeWidth, setSubTreeWidth] = useState(restored.subTreeWidth);
  // The Track Filter bar. Its open state is kept across runs, its picks are
  // not: a library that comes back already narrowed reads as a broken one.
  const [filterOpen, setFilterOpen] = useState(restored.filterOpen);
  const [filterState, setFilterState] = useState<FilterState>(EMPTY_FILTER);
  const [filterValues, setFilterValues] = useState<FilterValues | null>(null);
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
  // so the player reflects what is highlighted rather than nothing. It starts
  // empty on every run: a track restored from the last session would be loaded
  // before the library is read, which fails, and the deck would open on an
  // error over something nobody asked to hear.
  const [playerTrack, setPlayerTrack] = useState<RowDto | null>(null);
  // Deck B, which only exists in the two-player layouts. Loaded by dropping a
  // track on it: the browser's selection drives deck A alone, or picking
  // through a playlist would keep replacing whatever B was cued to.
  const [playerTrackB, setPlayerTrackB] = useState<RowDto | null>(null);
  // The one row the browser has selected, so an empty deck can be clicked to
  // take it. Selecting still loads nothing by itself.
  const [selectedRow, setSelectedRow] = useState<RowDto | null>(null);
  // Where each deck's transport is drawn in the two-deck layouts. State rather
  // than a ref, because the players have to re-render once the slots exist.
  const [transportA, setTransportA] = useState<HTMLDivElement | null>(null);
  const [transportB, setTransportB] = useState<HTMLDivElement | null>(null);
  /**
   * DUAL CONTROL: one zoom and one beat-jump size for both decks.
   *
   * Off, each deck keeps its own; on, the shell holds them and hands the same
   * value to both, so a wheel over one waveform moves the other with it.
   */
  /**
   * Which deck the other syncs to.
   *
   * Deck A to begin with, because that is the one a single-player layout has
   * and the one a first track lands on. Only ever one, which is what MASTER
   * means.
   */
  const [syncMaster, setSyncMasterState] = useState<DeckId>("a");
  /**
   * BEAT SYNC held on, per deck. A deck follows the master's tempo for as
   * long as its button is lit; the master itself never follows, so making a
   * deck the master puts its own light out.
   */
  const [synced, setSynced] = useState<Record<DeckId, boolean>>({ a: false, b: false });
  const setSyncMaster = useCallback((deck: DeckId) => {
    setSyncMasterState(deck);
    setSynced((s) => (s[deck] ? { ...s, [deck]: false } : s));
  }, []);
  const toggleSync = useMemo(
    () => ({
      a: () => setSynced((s) => ({ ...s, a: !s.a })),
      b: () => setSynced((s) => ({ ...s, b: !s.b })),
    }),
    [],
  );
  /**
   * What each deck is playing at — its file's BPM times its tempo — so the
   * synced deck can be handed the master's and re-match when it moves.
   */
  const [playingBpm, setPlayingBpm] = useState<Record<DeckId, number | null>>({ a: null, b: null });
  const reportPlayingBpm = useMemo(
    () => ({
      a: (bpm: number | null) => setPlayingBpm((p) => (p.a === bpm ? p : { ...p, a: bpm })),
      b: (bpm: number | null) => setPlayingBpm((p) => (p.b === bpm ? p : { ...p, b: bpm })),
    }),
    [],
  );
  const leaderBpmX100 = playingBpm[syncMaster];
  /**
   * How each deck reads the other for sync.
   *
   * Getters rather than state: a deck's position moves every frame and sync
   * reads it once, when the button goes down. Holding it as state would
   * re-render the shell sixty times a second for a number nobody is looking
   * at.
   */
  const syncA = useRef<() => SyncDeck | null>(() => null);
  const syncB = useRef<() => SyncDeck | null>(() => null);
  const publishSync = useMemo(
    () => ({
      a: (get: () => SyncDeck | null) => {
        syncA.current = get;
      },
      b: (get: () => SyncDeck | null) => {
        syncB.current = get;
      },
    }),
    [],
  );
  const peerSync = useMemo(
    () => ({ a: () => syncB.current(), b: () => syncA.current() }),
    [],
  );
  /**
   * The zoom cluster the two-deck layout shares, registered the same way:
   * one + RST − over the line where the two details meet, and a press
   * zooms both decks. DUAL CONTROL off, each deck still keeps its own zoom
   * for the wheel; the cluster is simply pressed on both.
   */
  const zoomA = useRef<(by: number) => void>(() => {});
  const zoomB = useRef<(by: number) => void>(() => {});
  const publishZoom = useMemo(
    () => ({
      a: (zoom: (by: number) => void) => {
        zoomA.current = zoom;
      },
      b: (zoom: (by: number) => void) => {
        zoomB.current = zoom;
      },
    }),
    [],
  );
  const zoomBoth = useCallback((by: number) => {
    zoomA.current(by);
    zoomB.current(by);
  }, []);
  const [dual, setDual] = useState(false);
  const [dualBars, setDualBars] = useState(DETAIL_BARS);
  const [dualJump, setDualJump] = useState(JUMP_SIZE_ID);
  const linked = dual
    ? {
        bars: dualBars,
        onBars: setDualBars,
        jumpSize: dualJump,
        onJumpSize: setDualJump,
      }
    : {};
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
        : selectedNode?.kind === "directory" || selectedNode?.kind === "explorer"
          ? "folder"
          : "collection";
  const cols = useColumns(columnContext);
  // The Preferences window, and the pane it opens on: the missing-file
  // manager lives under Advanced, so the File menu opens it there. In the
  // shell it is a window of its own; in a browser, which has no windows to
  // open, it is drawn over this one.
  const [settingsOpen, setSettingsOpen] = useState<Pane | null>(null);
  const openPreferences = useCallback((pane: Pane) => {
    void getBackend().then(async (backend) => {
      const opened = await backend.openPreferences(pane).catch(() => false);
      if (!opened) setSettingsOpen(pane);
    });
  }, []);
  const prefs = usePreferencesStore();
  const { view: viewPrefs, advanced: advancedPrefs, analysis: analysisPrefs } = prefs.preferences;
  // Every write path reads this one flag: rekordbox holding the database,
  // or Library Protection in Preferences, refuse the same way.
  const readOnly = (summary?.readOnly ?? false) || advancedPrefs.protectLibrary;
  // Checks on its own a while after launch when Preferences says so; the
  // menu and Preferences ask by hand.
  const updater = useUpdater(advancedPrefs.checkUpdates);
  const checkForUpdates = updater.check;
  // DJ System in Preferences is what a stick with no settings of its own
  // gets on export; the same shape goes with every export call.
  const stickDefaults = prefs.preferences.djSystem;
  /** How much of the window the deck takes, kept across restarts. */
  const [layout, setLayout] = useState<PlayerLayout>(restored.layout);
  /**
   * The Traffic Light: which deck's key the browser lights rows against —
   * the MASTER menu above the track list. The key itself is that deck's
   * loaded track's, so it follows the master when the master moves.
   */
  const [trafficLight, setTrafficLight] = useState<TrafficLightSource>(restored.trafficLight);
  const trafficDeck: DeckId = trafficLight === "master" ? syncMaster : trafficLight;
  const trafficKey = (trafficDeck === "b" ? playerTrackB : playerTrack)?.key ?? null;
  const master = useMaster();
  // Read at start so the remembered setting reaches the engine before the
  // first thing plays, not when Settings is next opened.
  const limiter = useLimiter();
  // An analysed track's waveform and key change, so its row is stale.
  const analysis = useAnalysis(
    useCallback((id: string) => {
      setPendingEdits((edits) => new Map(edits).set(id, { analysed: 1 }));
    }, []),
  );
  // What is in flight out of the browser: a playlist takes the ids, a deck
  // takes the one row under the hand.
  const [draggedTracks, setDraggedTracks] = useState<TrackDrag | null>(null);
  /**
   * The last thing the app has to say, and whether it went wrong.
   *
   * One channel, two colours: "Added 3 tracks" and "rekordbox is running, so
   * the library is open read-only" arrive the same way and are not the same
   * kind of news.
   */
  const [note, setNote] = useState<{ text: string; failed: boolean } | null>(null);
  const [playerError, setPlayerError] = useState<string | null>(null);
  const report = useCallback((text: string) => setNote({ text, failed: false }), []);
  const refuse = useCallback((text: string) => setNote({ text, failed: true }), []);
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

  // The master player's BPM for the filter's `MASTER PLAYER ±` list: the
  // track on whichever deck is MASTER. `[ASSUME]` the track's own BPM, not
  // the deck's tempo-adjusted one — the tempo lives in the player and the
  // capture cannot say which rekordbox uses.
  const masterBpmX100 = (syncMaster === "b" ? playerTrackB : playerTrack)?.bpmX100 ?? null;
  const spec: ViewSpec = useMemo(() => {
    const base = specForNode(selectedNode, query, sortState, viewPrefs.keyDisplay);
    // Only while the bar is showing: hiding it puts the whole list back,
    // so a closed bar can never be silently narrowing the library.
    const filter = filterOpen ? toSpecFilter(filterState, masterBpmX100) : undefined;
    return filter ? { ...base, filter } : base;
  }, [selectedNode, sortState, query, filterOpen, filterState, masterBpmX100, viewPrefs.keyDisplay]);

  // What the bar's lists offer, from Rust, for the source and query alone.
  // Re-asked when either changes or the library does, and only while the bar
  // is open — a closed bar costs nothing.
  useEffect(() => {
    if (!filterOpen) return;
    let live = true;
    void (async () => {
      const backend = await getBackend();
      try {
        const values = await backend.filterValues(specForNode(selectedNode, query, null));
        if (live) setFilterValues(values);
      } catch {
        // The library is not up yet; the ready event re-runs this through
        // `libraryGeneration`.
      }
    })();
    return () => {
      live = false;
    };
  }, [filterOpen, selectedNode, query, libraryGeneration]);

  const handleSort = useCallback((column: SortColumn) => {
    setSortState((s) => nextSort(s, column));
  }, []);

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
      // Library Protection is a choice the backend cannot see, so it is
      // refused here, with the same words the menu uses. rekordbox holding
      // the database is the backend's to refuse, checked as the write starts.
      if (advancedPrefs.protectLibrary) {
        refuse(refusal(true));
        return;
      }
      const backend = await getBackend();
      try {
        await edit(backend);
        report(what);
      } catch (e) {
        refuse(e instanceof Error ? e.message : "That could not be saved.");
      }
    },
    [report, refuse, advancedPrefs.protectLibrary],
  );

  /** Shows an edit at once, so the interface does not wait on the reload. */
  const showPending = useCallback((id: string, patch: Partial<RowDto>) => {
    setPendingEdits((edits) => new Map(edits).set(id, { ...edits.get(id), ...patch }));
  }, []);

  /**
   * A file the Explorer lists that the library does not hold cannot be
   * edited: the write would match no row. Said once, here, for every edit.
   */
  const refuseLoose = useCallback(
    (id: string) => {
      if (!isLooseId(id)) return false;
      refuse("That file is not in the collection. Import it first.");
      return true;
    },
    [refuse],
  );

  const rateTrack = useCallback(
    (id: string, stars: number) => {
      if (refuseLoose(id)) return;
      showPending(id, { rating: stars });
      void runEdit(stars === 0 ? "Rating cleared." : `Rated ${stars} of 5.`, (b) =>
        b.edits.setTrackRating(id, stars),
      );
    },
    [runEdit, showPending, refuseLoose],
  );

  const commentTrack = useCallback(
    (id: string, comment: string) => {
      if (refuseLoose(id)) return;
      showPending(id, { comment });
      void runEdit("Comment saved.", (b) => b.edits.setTrackComment(id, comment));
    },
    [runEdit, showPending, refuseLoose],
  );

  const addDraggedTo = useCallback(
    (playlistId: string) => {
      const ids = draggedTracks?.ids;
      setDraggedTracks(null);
      if (!ids || ids.length === 0) return;
      if (ids.some(refuseLoose)) return;
      if (advancedPrefs.protectLibrary) {
        refuse(refusal(true));
        return;
      }
      void (async () => {
        const backend = await getBackend();
        try {
          await backend.edits.addTracksToPlaylist(playlistId, [...ids]);
          const name = tree.find((n) => n.id === playlistId)?.name ?? "the playlist";
          report(`Added ${ids.length} track${ids.length === 1 ? "" : "s"} to ${name}.`);
        } catch (e) {
          // The refusal that matters is Rekordbox holding the database; say so
          // rather than letting the drop look as if it worked.
          refuse(e instanceof Error ? e.message : "That could not be saved.");
        }
      })();
    },
    [draggedTracks, tree, report, refuse, refuseLoose, advancedPrefs.protectLibrary],
  );

  /**
   * Loading a dragged track into a deck.
   *
   * A pair of stable callbacks rather than one taking a deck id: `Player` is
   * memoized, and a closure built during render would give it a new prop every
   * frame the app draws.
   */
  const loadDroppedInto = useMemo(() => {
    const into = (put: (row: RowDto) => void) => () => {
      const row = draggedTracks?.row;
      setDraggedTracks(null);
      // Nothing is written: which track a deck is holding is not part of the
      // library, so this is the one drop that cannot be refused.
      if (row) put(row);
    };
    return { a: into(setPlayerTrack), b: into(setPlayerTrackB) };
  }, [draggedTracks]);

  /** The same three decks, loaded from the track menu or from a click. */
  const loadInto = useMemo(
    () => ({ a: setPlayerTrack, b: setPlayerTrackB }),
    [],
  );
  const loadTrack = useCallback(
    (deck: DeckId, row: RowDto) => loadInto[deck === "b" ? "b" : "a"](row),
    [loadInto],
  );
  const loadSelectedInto = useMemo(() => {
    if (!selectedRow) return { a: undefined, b: undefined };
    return {
      a: () => setPlayerTrack(selectedRow),
      b: () => setPlayerTrackB(selectedRow),
    };
  }, [selectedRow]);

  /**
   * The tree's context menu, and the track's.
   *
   * Every one of these writes, so each says why it could not rather than
   * looking as if it worked — the same rule the drop handler above follows.
   */
  const afterWrite = useCallback(
    async (said: string) => {
      const backend = await getBackend();
      // The tree now, so the node that was made is there when the note says
      // so. The generation is the backend's alone, announced as
      // `library:changed` after every write: a count bumped here as well
      // could land on the number the backend announces for the next edit,
      // and a generation that does not change is a page that is not
      // refetched — a rating lit for a moment and went out.
      setTree(await backend.playlistTree());
      report(said);
    },
    [report],
  );

  const write = useCallback(
    (run: (backend: Backend) => Promise<string>) => {
      void (async () => {
        const backend = await getBackend();
        try {
          await afterWrite(await run(backend));
        } catch (e) {
          refuse(e instanceof Error ? e.message : "That could not be saved.");
        }
      })();
    },
    [afterWrite, refuse],
  );

  const createPlaylistIn = useCallback(
    (node: TreeNode) => {
      // rekordbox's own default name, from german.lang, and the node the menu
      // was opened on says where — a folder holds it, a playlist's own
      // folder does, and one at the top goes at the top.
      write(async (backend) => {
        await backend.edits.createPlaylist("New playlist", parentFor(tree, node));
        return "Created New playlist.";
      });
    },
    [write, tree],
  );

  const createFolderIn = useCallback(
    (node: TreeNode) => {
      write(async (backend) => {
        await backend.edits.createFolder("New folder", parentFor(tree, node));
        return "Created New folder.";
      });
    },
    [write, tree],
  );

  const deleteNode = useCallback(
    (node: TreeNode) => {
      write(async (backend) => {
        await backend.edits.deletePlaylist(node.id);
        if (selectedNode?.id === node.id) setSelectedNode(null);
        return `Deleted ${node.name}.`;
      });
    },
    [write, selectedNode],
  );

  const removeFromPlaylist = useCallback(
    (ids: readonly string[]) => {
      const playlist = spec.source.kind === "playlist" ? spec.source.id : null;
      if (playlist === null || ids.length === 0) return;
      write(async (backend) => {
        await backend.edits.removeTracksFromPlaylist(playlist, [...ids]);
        return `Removed ${ids.length} track${ids.length === 1 ? "" : "s"}.`;
      });
    },
    [write, spec],
  );

  const revealTrack = useCallback((row: RowDto) => {
    void (async () => {
      const backend = await getBackend();
      try {
        await backend.revealTrack(row.id);
      } catch (e) {
        refuse(e instanceof Error ? e.message : "That file could not be shown.");
      }
    })();
  }, [refuse]);

  // Clear the note after a moment: it reports an action, not a state.
  useEffect(() => {
    if (note === null) return;
    const timer = setTimeout(() => setNote(null), 4000);
    return () => {
      clearTimeout(timer);
    };
  }, [note]);

  /** Queues whatever is selected in the browser. */
  const analyseSelection = useCallback(() => {
    if (selectedTracks.length === 0) return;
    analysis.add(selectedTracks);
  }, [analysis, selectedTracks]);

  const importFromMenu = useCallback(async () => {
    report("Choosing files to import…");
    try {
      const backend = await getBackend();
      const imported = await backend.importFiles();
      if (imported === null) {
        setNote(null);
        return;
      }
      const total = imported.imported + imported.skipped.length;
      report(
        imported.skipped.length === 0
          ? `Imported ${imported.imported} of ${total} files.`
          : `Imported ${imported.imported} of ${total} files; ${imported.skipped.length} skipped.`,
      );
      setTree(await backend.playlistTree());
      // Auto Analysis in Preferences: what just landed goes straight into
      // the queue, as rekordbox does unless told not to.
      if (analysisPrefs.auto && imported.tracks.length > 0) analysis.add(imported.tracks);
    } catch (e) {
      refuse(e instanceof Error ? e.message : "Those files could not be imported.");
    }
  }, [report, refuse, analysisPrefs.auto, analysis]);

  // A menu item, by id. The shell sends the id and nothing else; what it
  // means, and whether it is allowed right now, is decided in one place —
  // and the keyboard reaches it the same way on the platforms where the
  // webview keeps the accelerators from the native menu.
  const runMenu = useCallback((id: string) => {
    const outcome = resolveMenu(id, readOnly, advancedPrefs.protectLibrary);
    if (!outcome) return;
    if ("refused" in outcome) {
      refuse(outcome.refused);
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
    if (outcome.action === "updates") {
      checkForUpdates(true);
      return;
    }
    if (outcome.action.startsWith("layout-")) {
      setLayout(asLayout(outcome.action.slice("layout-".length)));
      return;
    }
    // The missing-file manager is a pane of Preferences.
    openPreferences(outcome.action === "missing" ? "advanced" : "view");
  }, [readOnly, advancedPrefs.protectLibrary, importFromMenu, refuse, openPreferences, checkForUpdates]);

  // Native menu clicks.
  useEffect(() => {
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      stop = backend.onMenu(runMenu);
    })();
    return () => stop?.();
  }, [runMenu]);

  // The keyboard: the shell's own shortcuts, and the menu accelerators the
  // webview keeps from the native menu on Windows.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // The native menu's accelerators, on the platforms where a keystroke
      // in the webview never reaches them (see `menuAccelerator`).
      const item = menuAccelerator(event, platform);
      if (item !== null) {
        event.preventDefault();
        runMenu(item);
        return;
      }
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
  }, [platform, query, runMenu]);

  // The Preferences window asking for what only this window holds.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let live = true;
    void getBackend().then((backend) => {
      if (!live) return;
      stop = backend.onPreferencesReset((what) => {
        if (what === "updates") {
          // The Update Manager is this window's; Preferences gets out of
          // its way, as it would if it were a window of its own.
          setSettingsOpen(null);
          checkForUpdates(true);
        } else if (what === "columns") {
          cols.reset();
        } else {
          setTreeWidth(clampWidth(305, bounds()));
          setSubWidth(DEFAULT_SUB_WIDTH);
          setSubTreeWidth(DEFAULT_SUB_TREE_WIDTH);
        }
      });
    });
    return () => {
      live = false;
      stop?.();
    };
  }, [cols, bounds, checkForUpdates]);

  const refreshDevices = useCallback(() => {
    void (async () => {
      const backend = await getBackend();
      setDevices(await backend.listDevices());
    })();
  }, []);

  // Devices join the tree as nodes so the Devices section renders through the
  // same path as every other section, and the Explorer's folders after them.
  const explorer = useExplorer();
  // View › Layout in Preferences decides whether All Tracks heads the
  // playlists and whether the Explorer is there at all; the rail dims a
  // section with nothing in it, so a hidden Explorer reads as empty.
  const treeNodes = useMemo(
    () => [
      ...(viewPrefs.allTracks ? tree : tree.filter((node) => node.kind !== "allTracks")),
      ...deviceNodes(devices),
      ...(viewPrefs.explorer ? explorer.nodes : []),
    ],
    [tree, devices, explorer.nodes, viewPrefs.allTracks, viewPrefs.explorer],
  );
  const selectedDevice = useMemo(
    () => devices.find((device) => deviceId(device) === selectedNode?.id) ?? null,
    [devices, selectedNode],
  );

  const syncToDevice = useCallback(
    async (playlistId: string) => {
      if (!selectedDevice) return;
      const name = tree.find((n) => n.id === playlistId)?.name ?? "the playlist";
      setSyncing(true);
      report(`Writing ${name} to ${selectedDevice.name}…`);
      try {
        const backend = await getBackend();
        const written = await backend.exportPlaylist(playlistId, selectedDevice.path, stickDefaults);
        if (written !== null) report(exportSummary(selectedDevice.name, written));
        setDevices(await backend.listDevices());
      } catch (e) {
        refuse(e instanceof Error ? e.message : "That export could not be written.");
      } finally {
        setSyncing(false);
      }
    },
    [selectedDevice, tree, report, refuse, stickDefaults],
  );

  const exportPlaylist = useCallback((node: TreeNode) => {
    void (async () => {
      const backend = await getBackend();
      report(`Exporting ${node.name}…`);
      try {
        const written = await backend.exportPlaylist(node.id, undefined, stickDefaults);
        if (written === null) {
          setNote(null);
          return;
        }
        report(exportSummary(node.name, written));
      } catch (e) {
        refuse(e instanceof Error ? e.message : "That export could not be written.");
      }
    })();
  }, [report, refuse, stickDefaults]);

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
      filterOpen,
      // Only once the real library is up: storing the seed back over itself
      // would keep the first run's rows alive forever.
      tree: [...tree].slice(0, SEEDED_NODES),
      rows: screen.rows.slice(0, SEEDED_ROWS),
      count: screen.count,
      layout,
      subWidth,
      subTreeWidth,
      trafficLight,
    });
  }, [treeWidth, selectedNode, sortState, infoOpen, subOpen, filterOpen, tree, screen, layout, subWidth, subTreeWidth, trafficLight]);

  // The last screen, handed to the table until the backend answers. Dropped as
  // soon as the library is up, so a stale row cannot outlive its replacement —
  // and dropped when the library has failed, since a window that says the
  // library could not be opened must not go on showing last week's tracks
  // under that message (seen on a machine with no rekordbox at all).
  const seed = useMemo(
    () =>
      summary === null && loadError === null && restored.rows.length > 0
        ? { count: restored.count, rows: restored.rows }
        : undefined,
    [summary, loadError, restored.count, restored.rows],
  );

  const selectionText =
    selectedCount > 1 ? `Selected: ${selectedCount} Tracks` : selectedCount === 1 ? "Selected: 1 Track" : "";

  const tip = useTooltip();
  return (
    <PreferencesProvider value={prefs}>
    <div className={styles.window}>
      <div
        className={styles.titleBar}
        data-testid="title-bar"
        onMouseDown={startWindowDrag}
        onDoubleClick={toggleWindowMaximise}
      >
        {/* What the app is costing, in the corner: its own component, so a
            reading does not re-render the window around it. */}
        <AppCost className={styles.cost} />
        <span className={styles.appName}>rekordbox lite</span>
      </div>
      <TopBar
        clock={clock}
        onOpenSettings={() => openPreferences("view")}
        layout={layout}
        onLayoutChange={setLayout}
        level={master.level}
        onLevelChange={master.setLevel}
        peakLeft={master.peakLeft}
        peakRight={master.peakRight}
      />
      {/* Full Browser draws no deck at all, and no gutter under one. */}
      {deckCount(layout) > 0 ? (
        <div
          className={styles.decks}
          data-mixer={deckCount(layout) > 1 ? "" : undefined}
        >
          {/* One transport column for the pair, as rekordbox draws it: deck A
              down from the top, deck B up from the bottom, and the mixer strip
              beside it. Each deck still owns its own transport — it is
              portalled into the half of the column that belongs to it. */}
          {deckCount(layout) > 1 ? (
            <div className={styles.deckRail}>
              <div className={styles.transportSlot} ref={setTransportA} />
              {/* DUAL CONTROL, on the centre line where the capture has it:
                  it links what the two decks are showing rather than what
                  they are playing, so it belongs between them. */}
              <button
                type="button"
                className={styles.dual}
                aria-label="Dual control"
                aria-pressed={dual}
                title={tip("Link the waveform controls and beat jump across both decks.")}
                data-on={dual || undefined}
                onClick={() => setDual((was) => !was)}
              >
                <LayoutDualIcon className={styles.dualGlyph} />
              </button>
              <div className={styles.transportSlot} ref={setTransportB} />
            </div>
          ) : null}
          {/* Only with two decks. The one-player layout has no mixer and no
              crossfader, which is what rekordbox does — and what keeps a deck
              nobody has touched a fader for playing at the level of its file. */}
          {deckCount(layout) > 1 ? <MixerStrip /> : null}
          <Player
            track={playerTrack}
            onEject={() => setPlayerTrack(null)}
            onError={setPlayerError}
            simple={!isFullDeck(layout)}
            dragging={draggedTracks !== null}
            onDropTrack={loadDroppedInto.a}
            onLoadSelected={loadSelectedInto.a}
            transportSlot={deckCount(layout) > 1 ? transportA : null}
            dual={deckCount(layout) > 1}
            publishZoom={deckCount(layout) > 1 ? publishZoom.a : undefined}
            {...(deckCount(layout) > 1 ? linked : {})}
            publishSync={publishSync.a}
            {...(deckCount(layout) > 1 ? { peerSync: peerSync.a } : {})}
            isMaster={syncMaster === "a"}
            onMaster={() => setSyncMaster("a")}
            synced={synced.a && syncMaster !== "a"}
            onSyncToggle={deckCount(layout) > 1 ? toggleSync.a : undefined}
            leaderBpmX100={syncMaster === "a" ? null : leaderBpmX100}
            onPlayingBpm={reportPlayingBpm.a}
            readOnly={readOnly}
          />
          {deckCount(layout) > 1 ? (
            <Player
              deck="b"
              track={playerTrackB}
              onEject={() => setPlayerTrackB(null)}
              onError={setPlayerError}
              simple={!isFullDeck(layout)}
              dragging={draggedTracks !== null}
              onDropTrack={loadDroppedInto.b}
              onLoadSelected={loadSelectedInto.b}
              transportSlot={transportB}
              flipped
              dual
              publishZoom={publishZoom.b}
              {...linked}
              publishSync={publishSync.b}
              peerSync={peerSync.b}
              isMaster={syncMaster === "b"}
              onMaster={() => setSyncMaster("b")}
              synced={synced.b && syncMaster !== "b"}
              onSyncToggle={toggleSync.b}
              leaderBpmX100={syncMaster === "b" ? null : leaderBpmX100}
              onPlayingBpm={reportPlayingBpm.b}
              readOnly={readOnly}
            />
          ) : null}
          {/* Over the line between the decks, where the capture floats it.
              After the decks, so it paints over both. */}
          {deckCount(layout) > 1 ? (
            <div className={styles.zoomSlot}>
              <DualZoom onZoom={zoomBoth} />
            </div>
          ) : null}
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
          onCreatePlaylist={createPlaylistIn}
          onCreateFolder={createFolderIn}
          onDeleteNode={deleteNode}
          readOnly={readOnly}
          onExpand={explorer.expand}
          showCounts={viewPrefs.playlistCounts}
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
            onError={refuse}
            busy={syncing}
          />
        ) : (
        <TrackTable
          spec={spec}
          onSortChange={handleSort}
          onSelectionChange={setSelectedCount}
          onSelectedTracks={setSelectedTracks}
          onAnalyse={analyseSelection}
          onShowInformation={(row) => {
            setPlayerTrack(row);
            setInfoOpen(true);
          }}
          onShowInFinder={revealTrack}
          onRemoveFromPlaylist={removeFromPlaylist}
          readOnly={readOnly}
          trafficLight={trafficLight}
          onTrafficLight={setTrafficLight}
          trafficKey={trafficKey}
          onFocusedRow={setPlayerTrack}
          onSelectedRow={setSelectedRow}
          onDragTracks={setDraggedTracks}
          players={deckCount(layout)}
          onLoadTrack={loadTrack}
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
          filterOpen={filterOpen}
          onToggleFilter={() => setFilterOpen((was) => !was)}
          filterBar={
            <TrackFilter
              state={filterState}
              onChange={setFilterState}
              values={filterValues}
              masterBpmX100={masterBpmX100}
            />
          }
        />
        )}
        {subOpen ? (
          <SubBrowser
            nodes={tree}
            libraryGeneration={libraryGeneration}
            width={subWidth}
            treeWidth={subTreeWidth}
            onWidthChange={setSubWidth}
            onTreeWidthChange={setSubTreeWidth}
            // Its tree and list take part in everything the main pair does:
            // a drag from either list lands on either tree, and its rows
            // load decks and take edits the same way.
            tree={{
              dragging: draggedTracks !== null,
              onDropTracks: addDraggedTo,
              onExport: exportPlaylist,
              onCreatePlaylist: createPlaylistIn,
              onCreateFolder: createFolderIn,
              onDeleteNode: deleteNode,
              readOnly: readOnly,
            }}
            list={{
              onDragTracks: setDraggedTracks,
              players: deckCount(layout),
              onLoadTrack: loadTrack,
              onShowInFinder: revealTrack,
              onRate: rateTrack,
              onComment: commentTrack,
              pendingEdits,
              readOnly: readOnly,
            }}
          />
        ) : null}
        {infoOpen ? (
          <InfoPanel
            // The browser's selection, as rekordbox's Information Window
            // follows it; the deck's track only when nothing is selected.
            track={selectedRow ?? playerTrack}
            readOnly={readOnly}
            libraryGeneration={libraryGeneration}
            onRate={rateTrack}
            onComment={commentTrack}
            onEdit={runEdit}
          />
        ) : null}
        <RightRail
          className={styles.rightRail}
          infoOpen={infoOpen}
          onToggleInfo={() => setInfoOpen((open) => !open)}
          subOpen={subOpen}
          onToggleSub={() => setSubOpen((open) => !open)}
        />
      </div>
      {updater.open ? (
        <UpdateManager
          state={updater.state}
          onCheck={() => updater.check(true)}
          onInstall={updater.install}
          onClose={updater.dismiss}
        />
      ) : null}
      {settingsOpen !== null ? (
        <Preferences
          summary={summary}
          limiter={limiter.limiter}
          onLimiterChange={limiter.set}
          reduction={master.reduction}
          initialPane={settingsOpen}
          onResetColumns={cols.reset}
          onResetLayout={() => {
            setTreeWidth(clampWidth(305, bounds()));
            setSubWidth(DEFAULT_SUB_WIDTH);
            setSubTreeWidth(DEFAULT_SUB_TREE_WIDTH);
          }}
          onClose={() => setSettingsOpen(null)}
        />
      ) : null}

      <StatusBar
        activity={
          analysis.running
            ? `Analyzing: ${analysis.state.done + analysis.state.failed.length + 1} of ${analysis.total}` +
              (analysis.state.current ? ` — ${analysis.state.current.title}` : "")
            : (note !== null && !note.failed
                ? note.text
                : (summary ? `${summary.trackCount} Tracks` : "Loading the library…"))
        }
        // Everything that went wrong, in one place and in red: the deck's
        // refusals, a library that would not open, and a write the library
        // turned down.
        error={playerError ?? loadError ?? (note?.failed === true ? note.text : null)}
        onCancelAnalysis={analysis.running ? analysis.cancel : undefined}
        analysisFailures={analysis.state.failed.length}
        selection={selectionText}
        readOnly={readOnly}
        protectedLibrary={advancedPrefs.protectLibrary}
      />
    </div>
    </PreferencesProvider>
  );
}
