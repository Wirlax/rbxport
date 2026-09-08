/**
 * In-browser stand-in for the Rust backend.
 *
 * Deliberately mirrors the real contract's *shape*: it owns the ordering, hands
 * out view handles, and only ever returns a window of rows. That way the UI is
 * written against the real constraints from day one, and `pnpm dev:mock` runs
 * the actual components with no Tauri, no database and no rekordbox.
 *
 * Its sort and search semantics are checked against the Rust view tests by a
 * parity test once `rbl-index` lands.
 */
import type {
  Backend, Edits, LibrarySummary, RowDto, SortColumn, TreeNode, ViewHandle, ViewSpec, WaveformKind,
} from "./types";
import { TREE_ROOT } from "./types";

const ARTISTS = [
  "MORTEN", "ARTBAT", "Meduza", "Vintage Culture", "Tujamo", "UMEK", "Kryder", "Joel Corry",
  "Carl Bee", "Pretty Pink", "Dommo", "Eric Prydz", "Sarah de Warren", "Timmy Trumpet",
  "Oliver Heldens", "The Temper Trap", "Wh0", "Benny Benassi", "Hayla", "Anyma",
];
const TITLES = [
  "Take Me Home", "The Abyss", "Love To Give", "Your Eyes", "Love is Gonna Save Us",
  "Sweet Disposition", "Edge Of The World", "Another World", "Something In The Air",
  "Daydream", "Up To My Head", "Walking On A Dream", "Safe With Me", "Renegade Master",
  "Gravity", "Airplane Mode", "Wasted Time", "Brighter Days", "Angels", "Goddess",
];
const MIXES = ["(Extended Mix)", "(Original Mix)", "(Extended Remix)", "(Radio Edit)", "(Club Mix)"];
const KEYS = ["Am", "Bm", "Cm", "Dm", "Em", "Fm", "Gm", "Abm", "Bbm", "Dbm", "Ebm", "F#m", "D", "A", "E"];
const GENRES = ["House", "Tech House", "Melodic House", "Techno", "Trance", "Progressive House", ""];
const LABELS = ["Spinnin'", "Musical Freedom", "Defected", "Armada", "Toolroom", "Drumcode", ""];

/**
 * How long the mock pretends analysis takes.
 *
 * Long enough that a queue can be watched and stopped, short enough that a
 * suite of them is not slow.
 */
const ANALYSIS_MS = 120;

/** Deterministic PRNG so every run, test and screenshot sees identical data. */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function makeRows(count: number): RowDto[] {
  const rnd = mulberry32(20260907);
  const rows: RowDto[] = new Array(count);
  for (let i = 0; i < count; i++) {
    const analysed = rnd() > 0.12 ? 1 : 0;
    const artist = ARTISTS[Math.floor(rnd() * ARTISTS.length)] ?? "";
    const key = analysed ? (KEYS[Math.floor(rnd() * KEYS.length)] ?? "") : "";
    const bpmX100 = analysed ? (120 + Math.floor(rnd() * 20)) * 100 : 0;
    const month = 1 + Math.floor(rnd() * 12);
    const day = 1 + Math.floor(rnd() * 28);
    rows[i] = {
      id: String(100000 + i),
      trackNo: i + 1,
      title: `${TITLES[Math.floor(rnd() * TITLES.length)]} ${MIXES[Math.floor(rnd() * MIXES.length)]}`,
      artist,
      album: rnd() > 0.6 ? "Single" : "",
      genre: GENRES[Math.floor(rnd() * GENRES.length)] ?? "",
      label: LABELS[Math.floor(rnd() * LABELS.length)] ?? "",
      comment: analysed && rnd() > 0.5 ? `${1 + Math.floor(rnd() * 12)}A - ${key.slice(0, 1)} - ${bpmX100 / 100}` : "",
      bpmX100,
      key,
      durationSec: 180 + Math.floor(rnd() * 240),
      rating: Math.floor(rnd() * 6),
      analysed,
      dateAdded: `2026-0${1 + Math.floor(rnd() * 9)}-${String(day).padStart(2, "0")}`,
      releaseDate: rnd() > 0.3 ? `2026-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}` : "",
      cues: analysed ? (rnd() > 0.5 ? "EFGH" : "ABCD") : "",
      artworkHue: Math.floor(rnd() * 360),
      // The mock has no files to serve, so every row falls back to the tint.
      hasArtwork: false,
    };
  }
  return rows;
}

const FOLDERS = ["CURRENT", "USB", "DOWNLOADS"];
const PLAYLISTS = [
  "Melodic Vox", "Hardstyle", "Drum and Bass", "Eurodance", "Latin", "Main", "Main: Vocal",
  "Melodic Techno", "Techno", "Trance", "Groovy", "Fun House", "House", "Tech House",
  "Special", "HOUSE CLASSIC", "TRIODE",
];

function makeTree(): TreeNode[] {
  const nodes: TreeNode[] = [
    { id: "all", name: "All Tracks", kind: "allTracks", depth: 0 },
    { id: "playlists", name: "Playlists", kind: "collection", depth: 0, expanded: true },
  ];
  let n = 0;
  for (const [fi, folder] of FOLDERS.entries()) {
    nodes.push({ id: `folder-${fi}`, name: folder, kind: "folder", depth: 1, expanded: fi < 2 });
    if (fi >= 2) continue;
    const take = fi === 0 ? 3 : PLAYLISTS.length - 3;
    for (let i = 0; i < take; i++) {
      const name = PLAYLISTS[n++ % PLAYLISTS.length] ?? "";
      nodes.push({ id: `pl-${fi}-${i}`, name, kind: "playlist", depth: 2 });
    }
  }
  return nodes;
}

const collator = new Intl.Collator("en", { sensitivity: "base", numeric: true });

function compare(a: RowDto, b: RowDto, col: SortColumn): number {
  switch (col) {
    case "trackNo": return a.trackNo - b.trackNo;
    case "bpm": return a.bpmX100 - b.bpmX100;
    case "duration": return a.durationSec - b.durationSec;
    case "rating": return a.rating - b.rating;
    case "title": return collator.compare(a.title, b.title);
    case "artist": return collator.compare(a.artist, b.artist);
    case "album": return collator.compare(a.album, b.album);
    case "genre": return collator.compare(a.genre, b.genre);
    case "label": return collator.compare(a.label, b.label);
    case "key": return collator.compare(a.key, b.key);
    case "dateAdded": return collator.compare(a.dateAdded, b.dateAdded);
    case "releaseDate": return collator.compare(a.releaseDate, b.releaseDate);
  }
}

/** Matches the folding the Rust index uses: lowercase, accents stripped. */
export function fold(s: string): string {
  return s.normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

export interface MockOptions {
  trackCount?: number;
  /** Simulated IPC latency in ms; 0 keeps tests fast. */
  latencyMs?: number;
}

export function createMockBackend(options: MockOptions = {}): Backend {
  const trackCount = options.trackCount ?? readCountFromUrl() ?? 2000;
  const latency = options.latencyMs ?? 0;
  const all = makeRows(trackCount);
  const folded = all.map((r) => fold(`${r.title} ${r.artist} ${r.album} ${r.comment}`));
  const tree = makeTree();

  const views = new Map<number, { order: Uint32Array; gen: number }>();
  let nextViewId = 1;

  // The mock owns the tree the same way Rust does, so the edit flows can be
  // driven end to end in `pnpm dev:mock` and in Playwright without a database.
  let generation = 1;
  const membership = new Map<string, string[]>();
  let nextId = 1;

  const listeners = new Set<(generation: number) => void>();

  /**
   * Every edit bumps the generation and tells anyone listening, exactly as a
   * real write does — the backend reloads and emits `library:changed`.
   */
  const bump = (): Promise<number> => {
    generation += 1;
    for (const listener of listeners) listener(generation);
    return wait(generation);
  };

  const findNode = (id: string) => tree.find((n) => n.id === id);

  const edits: Edits = {
    createPlaylist: (name, parent) => {
      const depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      tree.push({ id: `made-${nextId++}`, name, kind: "playlist", depth });
      return bump();
    },
    createFolder: (name, parent) => {
      const depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      tree.push({ id: `made-${nextId++}`, name, kind: "folder", depth, expanded: true });
      return bump();
    },
    renamePlaylist: (id, name) => {
      const node = findNode(id);
      if (node) node.name = name;
      return bump();
    },
    movePlaylist: (id, parent) => {
      const node = findNode(id);
      if (node) node.depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      return bump();
    },
    deletePlaylist: (id) => {
      const at = tree.findIndex((n) => n.id === id);
      if (at >= 0) tree.splice(at, 1);
      membership.delete(id);
      return bump();
    },
    addTracksToPlaylist: (playlist, tracks) => {
      // The real backend refuses while Rekordbox holds the database; the mock
      // never does, so the happy path is what `pnpm dev:mock` exercises.
      const current = membership.get(playlist) ?? [];
      for (const track of tracks) if (!current.includes(track)) current.push(track);
      membership.set(playlist, current);
      return bump();
    },
    removeTracksFromPlaylist: (playlist, tracks) => {
      const current = (membership.get(playlist) ?? []).filter((t) => !tracks.includes(t));
      membership.set(playlist, current);
      return bump();
    },
    reorderPlaylist: (playlist, tracks) => {
      const current = membership.get(playlist) ?? [];
      // Mirrors the backend: tracks not named keep their place after the rest,
      // so a partial order cannot silently drop any.
      const named = tracks.filter((t) => current.includes(t));
      membership.set(playlist, [...named, ...current.filter((t) => !named.includes(t))]);
      return bump();
    },
    setTrackRating: (track, stars) => {
      const row = all.find((r) => r.id === track);
      if (row) row.rating = Math.max(0, Math.min(5, stars));
      return bump();
    },
    setTrackComment: (track, comment) => {
      const row = all.find((r) => r.id === track);
      if (row) row.comment = comment;
      return bump();
    },
    setTrackColor: (track, color) => {
      const row = all.find((r) => r.id === track);
      if (row) row.artworkHue = color === null ? 0 : Number.parseInt(color, 10) * 40;
      return bump();
    },
  };

  const wait = <T>(value: T): Promise<T> =>
    latency > 0 ? new Promise((r) => setTimeout(() => r(value), latency)) : Promise.resolve(value);

  return {
    librarySummary: () =>
      wait<LibrarySummary>({
        trackCount,
        playlistCount: tree.filter((n) => n.kind === "playlist").length,
        readOnly: true,
        dbVersion: null,
      }),

    // A copy, like the real backend: handing out the internal array lets a
    // caller mutate the backend's own state, and makes a list captured before
    // an edit appear to have changed by itself.
    playlistTree: () => wait(tree.map((node) => ({ ...node }))),

    openView: (spec: ViewSpec) => {
      // Playlists show a deterministic slice so the mock stays stable across runs.
      let candidates: number[];
      if (spec.source.kind === "playlist") {
        const seed = [...spec.source.id].reduce((a, c) => a + c.charCodeAt(0), 0);
        const size = 14 + (seed % 30);
        candidates = Array.from({ length: size }, (_, i) => (seed * 37 + i * 101) % trackCount);
      } else {
        candidates = Array.from({ length: trackCount }, (_, i) => i);
      }

      const q = fold(spec.query.trim());
      if (q) candidates = candidates.filter((i) => (folded[i] ?? "").includes(q));

      const order = Uint32Array.from(candidates);
      const rows = all;
      const sorted = Array.from(order).sort((x, y) => {
        const rx = rows[x];
        const ry = rows[y];
        if (!rx || !ry) return 0;
        const c = compare(rx, ry, spec.sort);
        return spec.descending ? -c : c;
      });

      const viewId = nextViewId++;
      views.set(viewId, { order: Uint32Array.from(sorted), gen: 1 });
      return wait<ViewHandle>({ viewId, len: sorted.length, gen: 1 });
    },

    fetchRows: (viewId, offset, len) => {
      const view = views.get(viewId);
      if (!view) return Promise.reject(new Error(`unknown view ${viewId}`));
      const out: RowDto[] = [];
      const end = Math.min(view.order.length, offset + len);
      for (let i = Math.max(0, offset); i < end; i++) {
        const row = all[view.order[i] ?? 0];
        if (row) out.push(row);
      }
      return wait(out);
    },

    trackWaveform: (trackId: string, _kind: WaveformKind) => {
      // Synthesised so the mock exercises the same drawing path as real data:
      // one byte per column, low five bits height, top three whiteness.
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0) return wait(new Uint8Array());
      const rnd = mulberry32(index + 1);
      const columns = 400;
      const out = new Uint8Array(columns);
      for (let i = 0; i < columns; i++) {
        const shape = 0.35 + 0.65 * Math.abs(Math.sin((i / columns) * Math.PI * 3));
        const height = Math.round(shape * (0.6 + rnd() * 0.4) * 31);
        const whiteness = Math.round(rnd() * 7);
        out[i] = (whiteness << 5) | (height & 0x1f);
      }
      return wait(out);
    },

    viewIdsInRange: (viewId, from, to) => {
      const view = views.get(viewId);
      if (!view) return Promise.reject(new Error(`unknown view ${viewId}`));
      const [lo, hi] = from <= to ? [from, to] : [to, from];
      const out: string[] = [];
      for (let i = Math.max(0, lo); i <= Math.min(view.order.length - 1, hi); i++) {
        const row = all[view.order[i] ?? 0];
        if (row) out.push(row.id);
      }
      return wait(out);
    },

    edits,

    // A steady grid at the track's own BPM, so the detail waveform has beats
    // to draw without an analysis file behind it.
    trackBeats: (trackId, fromMs, toMs) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0 || row.bpmX100 === 0) return wait([]);
      const beatMs = (60 / (row.bpmX100 / 100)) * 1000;
      const beats: { timeMs: number; downbeat: boolean }[] = [];
      const first = Math.max(0, Math.floor(fromMs / beatMs));
      for (let n = first; n * beatMs <= toMs && beats.length < 2000; n++) {
        beats.push({ timeMs: Math.round(n * beatMs), downbeat: n % 4 === 0 });
      }
      return wait(beats);
    },

    // A memory cue and four hot cues, so the player's markers and list have
    // something to draw without a database behind them.
    trackCues: (trackId) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0) return wait([]);
      const total = row.durationSec * 1000;
      return wait([
        { positionMs: Math.round(total * 0.02), letter: "", memory: true },
        { positionMs: Math.round(total * 0.12), letter: "A", memory: false },
        { positionMs: Math.round(total * 0.34), letter: "B", memory: false },
        { positionMs: Math.round(total * 0.61), letter: "C", memory: false },
        { positionMs: Math.round(total * 0.83), letter: "D", memory: false },
      ]);
    },

    // No picker and no filesystem in a browser, so nothing can be written.
    exportPlaylist: () => wait(null),

    // No network in a browser, so there is nothing to listen to. Saying why
    // is better than a panel that silently shows nothing.
    startLinkListening: () =>
      wait({
        listening: false,
        problem: "Link needs the desktop application; a browser has no access to the network.",
        peers: [],
      }),
    stopLinkListening: () => wait(undefined),
    onLinkPeers: () => () => undefined,

    // Analysis is real work in the app; here it just answers, so the queue's
    // sequencing and progress can be driven end to end without audio.
    analyseTrack: (trackId) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row) return Promise.reject(new Error("That track is not in the library."));
      // Every seventh track fails, so the failure path is exercised too.
      if (index % 7 === 6) {
        return Promise.reject(new Error("That file could not be decoded."));
      }
      row.analysed = 1;
      // Deliberately not instant. Real analysis is a decode and a DSP pass —
      // seconds per track — and a mock that answers immediately makes the
      // queue's progress, cancellation and failure handling unobservable.
      return new Promise((resolve) =>
        setTimeout(
          () => resolve({
            trackId,
            bpmX100: row.bpmX100 || 12_800,
            key: row.key || "Am",
            elapsedMs: ANALYSIS_MS,
          }),
          ANALYSIS_MS,
        ),
      );
    },

    // No picker in a browser, so nothing can be chosen to import.
    importFiles: () => wait(null),

    // Nothing in the mock has a file behind it, so nothing can be missing and
    // there is no picker to choose one with.
    missingTracks: () => wait({ total: 0, tracks: [] }),
    relocateTrack: () => wait(null),

    onLibraryChanged: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** `?tracks=40000` lets the perf spec load a full-size library into the mock. */
function readCountFromUrl(): number | null {
  if (typeof location === "undefined") return null;
  const raw = new URLSearchParams(location.search).get("tracks");
  const n = raw ? Number.parseInt(raw, 10) : NaN;
  return Number.isFinite(n) && n > 0 && n <= 200000 ? n : null;
}
