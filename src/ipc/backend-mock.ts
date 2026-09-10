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
  AppErrorDto, Backend, Cue, DeckEvent, Device, DeviceSettings, Edits, FilterValues,
  LibrarySummary, RowDto, SortColumn, Tick, TrackFilter, TreeNode, ViewHandle, ViewSpec,
  WaveformKind,
} from "./types";
import { TREE_ROOT } from "./types";
import { toCamelot } from "@/lib/camelot";
import { COLOR_NAMES, wholeBpm } from "@/lib/trackFilter";
import { referenceDeviceSettings } from "./mock-device-settings";

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

/**
 * Each row's `ColorID`, 0 for none and 1 to 8 for the colour comments.
 *
 * Beside the rows rather than on them: the row DTO carries no colour yet, and
 * the filter bar needs one to filter by. Roughly one row in six is coloured,
 * so a ticked colour narrows the list visibly without emptying it.
 */
function makeColors(count: number): Uint8Array {
  const rnd = mulberry32(20260909);
  const out = new Uint8Array(count);
  for (let i = 0; i < count; i++) {
    out[i] = rnd() > 0.85 ? 1 + Math.floor(rnd() * 8) : 0;
  }
  return out;
}

/**
 * The filter bar's semantics, as `rbl-index` has them.
 *
 * Whole BPMs at ±0% match by rounded bucket; a tolerance widens each picked
 * BPM into a band, or with `All` picked, a band around the master player —
 * and with no master player the BPM column matches everything. The other
 * columns are sets, and every ticked column has to agree.
 */
function passesFilter(row: RowDto, color: number, filter: TrackFilter): boolean {
  if (filter.bpm) {
    const { values, tolerancePct, masterBpmX100 } = filter.bpm;
    const within = (centre: number) =>
      Math.abs(row.bpmX100 - centre) <= Math.floor((centre * tolerancePct) / 100);
    if (values.length > 0) {
      const hit =
        tolerancePct === 0
          ? values.includes(wholeBpm(row.bpmX100))
          : values.some((v) => within(v * 100));
      if (!hit) return false;
    } else if (masterBpmX100 !== null && masterBpmX100 > 0 && !within(masterBpmX100)) {
      return false;
    }
  }
  if (filter.keys && !filter.keys.includes(row.key)) return false;
  if (filter.ratings && !filter.ratings.includes(row.rating)) return false;
  if (filter.colors && !filter.colors.includes(COLOR_NAMES[color - 1] ?? "")) return false;
  return true;
}

/** Camelot number then letter, unknown names last: the order the bar lists keys in. */
function keyOrder(key: string): [number, number, string] {
  const code = toCamelot(key);
  if (!code) return [99, 2, key.toLowerCase()];
  return [Number(code.slice(0, -1)), code.endsWith("A") ? 0 : 1, ""];
}

function makeRows(count: number): RowDto[] {
  const rnd = mulberry32(20260907);
  const rows: RowDto[] = Array.from({ length: count });
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
  // Histories, filed as rekordbox files them: a folder per year, one per month
  // inside it, and the sessions under that. Closed, because the real library
  // has 187 of them and they would otherwise open over the playlists.
  nodes.push({ id: "histories", name: "Histories", kind: "histories", depth: 0, expanded: false });
  nodes.push({ id: "hist-2026", name: "2026", kind: "history", depth: 1, expanded: false });
  nodes.push({ id: "hist-202609", name: "9", kind: "history", depth: 2, expanded: false });
  for (const day of ["2026-09-04", "2026-08-30", "2026-08-23"]) {
    nodes.push({ id: `hist-${day}`, name: `LINK HISTORY ${day}`, kind: "history", depth: 3 });
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
  /**
   * Whether the library reports itself writable. Off by default — the mock
   * stands in for a library rekordbox is holding, which is what the menus
   * and the deck's editing controls are tested against — and `?writable=1`
   * turns it on for the tests that exercise an edit.
   */
  writable?: boolean;
}

export function createMockBackend(options: MockOptions = {}): Backend {
  const trackCount = options.trackCount ?? readCountFromUrl() ?? 2000;
  const latency = options.latencyMs ?? readLatencyFromUrl() ?? 0;
  const writable = options.writable ?? readWritableFromUrl();
  const all = makeRows(trackCount);
  const colors = makeColors(trackCount);
  const folded = all.map((r) => fold(`${r.title} ${r.artist} ${r.album} ${r.comment}`));

  /** The rows a source and query leave, before sorting and before the filter. */
  const candidatesFor = (spec: ViewSpec): number[] => {
    // A playlist or a history session shows a deterministic slice, so the
    // mock stays stable across runs.
    let candidates: number[];
    if (spec.source.kind === "playlist" || spec.source.kind === "history") {
      const seed = [...spec.source.id].reduce((a, c) => a + c.charCodeAt(0), 0);
      const size = 14 + (seed % 30);
      candidates = Array.from({ length: size }, (_, i) => (seed * 37 + i * 101) % trackCount);
    } else {
      candidates = Array.from({ length: trackCount }, (_, i) => i);
    }
    const q = fold(spec.query.trim());
    if (q) candidates = candidates.filter((i) => (folded[i] ?? "").includes(q));
    return candidates;
  };
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

  /**
   * Whether the library has "finished loading".
   *
   * The mock answers instantly, which is exactly why the real app could sit on
   * "Loading…" forever without a test noticing: the backend loads on its own
   * thread and the first request can arrive before there is anything to answer
   * with. `?slow` holds the library back until `window.__libraryReady()` is
   * called, so that race can be driven deliberately.
   */
  let ready =
    typeof location === "undefined" || !new URLSearchParams(location.search).has("slow");
  const readyListeners = new Set<() => void>();
  if (typeof window !== "undefined") {
    (window as unknown as { __libraryReady: () => void }).__libraryReady = () => {
      ready = true;
      for (const listener of readyListeners) listener();
    };
  }

  /** What the backend says before the library is up. */
  const notReady = () =>
    Promise.reject(new Error("The library has not finished loading yet."));

  // A browser cannot see a real volume, so the mock carries one. It is
  // mutable: exporting to it changes what a later `listDevices` reports, which
  // is what makes the difference between a first export and a sync visible.
  const devices: Device[] = [
    {
      name: "DJ STICK",
      path: "/Volumes/DJ STICK",
      totalBytes: 32 * 1024 ** 3,
      freeBytes: 24 * 1024 ** 3,
      removable: true,
      export: null,
    },
    // A stick rekordbox wrote, as the device tabs' captures show one: the
    // real TEST stick's sizes and contents, so the panel can be checked
    // against them.
    {
      name: "TEST",
      path: "/Volumes/TEST",
      totalBytes: 1_535_800_000_000,
      freeBytes: 216_800_000_000,
      removable: true,
      export: { tracks: 77, playlists: 3, ours: false, written: "" },
    },
  ];

  // What each stick's tabs hold. DJ STICK starts empty and gains a library
  // when something is exported to it; TEST carries the rows read off the
  // real stick.
  const deviceSettings = new Map<string, DeviceSettings>([
    ["/Volumes/TEST", referenceDeviceSettings("TEST", true)],
  ]);
  // Handed out by copy, as IPC would: a caller mutating its copy must not
  // reach into the "stick". A few dozen small rows, nothing like a page.
  const copySettings = (s: DeviceSettings): DeviceSettings => ({
    ...s,
    categories: s.categories.map((slot) => ({ ...slot })),
    sorts: s.sorts.map((slot) => ({ ...slot })),
    colors: s.colors.map((c) => ({ ...c })),
  });
  const settingsOf = (path: string): DeviceSettings => {
    const known = deviceSettings.get(path);
    if (known) return known;
    const device = devices.find((d) => d.path === path);
    const fresh = referenceDeviceSettings(device?.name ?? "", device?.export !== null);
    deviceSettings.set(path, fresh);
    return fresh;
  };

  /** How many tracks a playlist holds — the same count `openView` shows. */
  const playlistSize = (id: string): number => {
    const seed = [...id].reduce((a, c) => a + c.charCodeAt(0), 0);
    return 14 + (seed % 30);
  };

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
      const at = all.findIndex((r) => r.id === track);
      const row = all[at];
      if (row) {
        row.artworkHue = color === null ? 0 : Number.parseInt(color, 10) * 40;
        colors[at] = color === null ? 0 : Number.parseInt(color, 10);
      }
      return bump();
    },
    addCue: (track, kind, positionMs) => {
      if (!all.some((r) => r.id === track)) return refuse(`no track ${track}`);
      if (kind !== "memory" && !/^[A-P]$/.test(kind.hot)) {
        return refuse(`${JSON.stringify(kind.hot)} is not a hot cue slot rekordbox has`);
      }
      const id = `cue-${nextCueId++}`;
      cuesOf(track).push({
        id, positionMs, outMs: 0, letter: kind === "memory" ? "" : kind.hot, memory: kind === "memory",
      });
      return cuesChanged(track, id);
    },
    addLoop: (track, kind, inMs, outMs) => {
      if (!all.some((r) => r.id === track)) return refuse(`no track ${track}`);
      if (outMs <= inMs) return refuse("a loop has to end after it starts");
      const id = `cue-${nextCueId++}`;
      cuesOf(track).push({
        id, positionMs: inMs, outMs, letter: kind === "memory" ? "" : kind.hot, memory: kind === "memory",
      });
      return cuesChanged(track, id);
    },
    moveCue: (cue, positionMs) => {
      const found = findCue(cue);
      if (!found) return notFound(`no cue ${cue}`);
      found.cue.positionMs = positionMs;
      return cuesChanged(found.track, undefined);
    },
    deleteCue: (cue) => {
      const found = findCue(cue);
      if (!found) return notFound(`no cue ${cue}`);
      const list = cuesOf(found.track);
      list.splice(list.indexOf(found.cue), 1);
      return cuesChanged(found.track, undefined);
    },
  };

  /*
   * Cues, per track, made up on first ask the way `trackCues` always did and
   * held from then on so an edit sticks. The same shape the real backend
   * keeps: the listeners hear which track changed, not a generation.
   */
  const cueStore = new Map<string, Cue[]>();
  let nextCueId = 1;
  const cueListeners = new Set<(trackId: string) => void>();
  const cuesOf = (trackId: string): Cue[] => {
    const held = cueStore.get(trackId);
    if (held) return held;
    const index = Number.parseInt(trackId, 10) - 100000;
    const row = all[index];
    const made: Cue[] = [];
    if (row && row.analysed !== 0) {
      const total = row.durationSec * 1000;
      made.push(
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.02), outMs: 0, letter: "", memory: true },
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.12), outMs: 0, letter: "A", memory: false },
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.34), outMs: 0, letter: "B", memory: false },
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.61), outMs: 0, letter: "C", memory: false },
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.83), outMs: 0, letter: "D", memory: false },
      );
    }
    cueStore.set(trackId, made);
    return made;
  };
  const findCue = (id: string): { track: string; cue: Cue } | null => {
    for (const [track, list] of cueStore) {
      const cue = list.find((c) => c.id === id);
      if (cue) return { track, cue };
    }
    return null;
  };
  const cuesChanged = <T>(track: string, value: T): Promise<T> => {
    for (const listener of cueListeners) listener(track);
    return wait(value);
  };
  // The shape a refused command arrives in: an `Error` whose `kind` is the
  // `AppErrorDto` kind, so a caller can tell a read-only refusal from a bug.
  const failed = (kind: AppErrorDto["kind"], message: string) =>
    Promise.reject(Object.assign(new Error(message), { kind }));
  const refuse = (message: string) => failed("readOnly", message);
  const notFound = (message: string) => failed("notFound", message);

  /*
   * The mock deck.
   *
   * `startClock` is a chain of timeouts rather than an interval: an interval
   * that outlives the page keeps firing, and the engine's own ticker likewise
   * stops the moment nothing is playing.
   */
  const SAMPLE_RATE = 44_100;
  const TICK_MS = 100;
  const deckA = {
    frames: 0, totalFrames: 0, generation: 0, playing: false, loaded: false,
    tempo: 1, masterTempo: false,
  };
  // Deck B holds its own tempo and key lock even though a browser has no
  // audio to apply them to: a control that snapped back on the next tick would
  // read as a broken one, and the deck it stands for does keep them.
  const deckB = {
    frames: 0, totalFrames: 0, generation: 0, playing: false, loaded: false,
    tempo: 1, masterTempo: false,
  };
  const deckTickListeners = new Set<(tick: Tick) => void>();
  const deckEventListeners = new Set<(event: DeckEvent) => void>();
  let clock: ReturnType<typeof setTimeout> | null = null;
  let clockAt = 0;

  /** The master level, which a browser can hold even with nothing to apply it to. */
  let master = 1;

  const tick = (): Tick => ({
    a: { ...deckA },
    b: { ...deckB },
    sampleRate: SAMPLE_RATE,
    // A browser has no audio callback, so there is nothing to meter. Zero is
    // the truth here rather than a placeholder: nothing is coming out.
    peakLeft: 0,
    peakRight: 0,
    master,
  });

  const sendTick = () => {
    const now = tick();
    for (const listener of deckTickListeners) listener(now);
  };

  const stopClock = () => {
    if (clock !== null) clearTimeout(clock);
    clock = null;
  };

  const startClock = () => {
    if (clock !== null) return;
    clockAt = performance.now();
    const step = () => {
      clock = null;
      const now = performance.now();
      deckA.frames = Math.min(
        deckA.frames + Math.round(((now - clockAt) / 1000) * SAMPLE_RATE),
        deckA.totalFrames,
      );
      clockAt = now;
      if (deckA.frames >= deckA.totalFrames) deckA.playing = false;
      sendTick();
      if (deckA.playing) clock = setTimeout(step, TICK_MS);
    };
    clock = setTimeout(step, TICK_MS);
  };

  const wait = <T>(value: T): Promise<T> =>
    latency > 0 ? new Promise((r) => setTimeout(() => r(value), latency)) : Promise.resolve(value);

  return {
    librarySummary: () =>
      ready
        ? wait<LibrarySummary>({
            trackCount,
            playlistCount: tree.filter((n) => n.kind === "playlist").length,
            readOnly: !writable,
            dbVersion: null,
          })
        : notReady(),

    // A copy, like the real backend: handing out the internal array lets a
    // caller mutate the backend's own state, and makes a list captured before
    // an edit appear to have changed by itself.
    playlistTree: () => (ready ? wait(tree.map((node) => ({ ...node }))) : notReady()),

    openView: (spec: ViewSpec) => {
      // Every real command goes through `state.library()?`, so none of them
      // answer before the library is up. A mock that served rows while the
      // summary was still failing would not be standing in for anything.
      if (!ready) return notReady();
      let candidates = candidatesFor(spec);
      const filter = spec.filter;
      if (filter) {
        candidates = candidates.filter((i) => {
          const row = all[i];
          return row ? passesFilter(row, colors[i] ?? 0, filter) : false;
        });
      }

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
        // The position in *this* view, which is what the `#` column shows and
        // what the real backend computes in `rows_to_dto`. Handing back the
        // row's stored number would show the collection's order inside a
        // playlist.
        if (row) out.push({ ...row, trackNo: i + 1 });
      }
      return wait(out);
    },

    trackWaveform: (trackId: string, kind: WaveformKind, window) => {
      // Three bytes a column — low, mid, high band energy — which is what the
      // real backend serves from `PWV6` and `PWV7`. Synthesising the old
      // one-byte `PWAV` shape here drew the bands from garbage.
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0) return wait(new Uint8Array());
      const rnd = mulberry32(index + 1);
      // `PWV6` is 1,200 columns for any track; `PWV7` is far denser.
      const columns = kind === "bandsDetail" ? 12_000 : 1200;
      const out = new Uint8Array(columns * 3);
      for (let i = 0; i < columns; i++) {
        const at = i / columns;
        // A shape with quiet intros and outros, so the overview reads as a
        // track rather than a block.
        const envelope = Math.min(1, Math.min(at, 1 - at) * 6) * (0.55 + 0.45 * Math.abs(Math.sin(at * Math.PI * 5)));
        const jitter = 0.75 + rnd() * 0.25;
        out[i * 3] = Math.round(envelope * jitter * 110);
        out[i * 3 + 1] = Math.round(envelope * jitter * 70);
        // Highs are sparse, which is what puts the bright core in the middle.
        out[i * 3 + 2] = Math.round(envelope * (rnd() > 0.7 ? rnd() * 45 : rnd() * 8));
      }
      if (!window) return wait(out);
      const first = Math.min(window.from, columns) * 3;
      const len = Math.min(window.len, columns - window.from) * 3;
      return wait(out.subarray(first, first + Math.max(0, len)));
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
    // to draw without an analysis file behind it. Encoded as the backend
    // encodes it: five bytes a beat, milliseconds then the beat's number.
    trackBeats: (trackId) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0 || row.bpmX100 === 0) return wait(new Uint8Array());
      const beatMs = (60 / (row.bpmX100 / 100)) * 1000;
      const count = Math.min(Math.floor((row.durationSec * 1000) / beatMs) + 1, 65536);
      const bytes = new Uint8Array(count * 5);
      const view = new DataView(bytes.buffer);
      for (let n = 0; n < count; n++) {
        view.setUint32(n * 5, Math.round(n * beatMs), true);
        view.setUint8(n * 5 + 4, (n % 4) + 1);
      }
      return wait(bytes);
    },

    // A memory cue and four hot cues, so the player's markers and list have
    // something to draw without a database behind them. A copy, ordered by
    // position as the index orders them, so an edit cannot reach the store.
    trackCues: (trackId) =>
      wait(cuesOf(trackId).map((cue) => ({ ...cue })).sort((a, b) => a.positionMs - b.positionMs)),

    // A plausible structure, so the phrase bar can be driven without an
    // analysis file: a real track's phrases tile it end to end.
    trackPhrases: (trackId) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0) return wait([]);
      const total = row.durationSec * 1000;
      const shape = [
        "INTRO 2", "CHORUS 1", "DOWN", "UP 1", "UP 3", "CHORUS 1",
        "DOWN", "UP 1", "CHORUS 1", "DOWN", "OUTRO",
      ];
      return wait(
        shape.map((label, i) => ({
          beat: i * 32,
          timeMs: Math.round((total * i) / shape.length),
          kind: i,
          label,
        })),
      );
    },

    // Vocals over the middle two thirds, so the strip has something to draw.
    trackVocals: (trackId) => {
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      if (!row || row.analysed === 0) return wait(new Uint8Array());
      const out = new Uint8Array(1200);
      for (let i = 0; i < out.length; i++) {
        const at = i / out.length;
        out[i] = at > 0.25 && at < 0.85 && Math.floor(at * 40) % 3 !== 0 ? 200 : 0;
      }
      return wait(out);
    },

    // No filesystem in a browser, so nothing is written — but the counts are
    // answered so the device panel's reporting can be driven end to end.
    exportPlaylist: (playlistId, destination) => {
      if (destination === undefined) return wait(null);
      const device = devices.find((d) => d.path === destination);
      const tracks = playlistSize(playlistId);
      const already = device?.export;
      const reused = already?.ours === true ? Math.min(already.tracks, tracks) : 0;
      if (device) {
        device.export = {
          tracks,
          playlists: 1,
          ours: true,
          written: "2026-09-08 00:30:00.000 +00:00",
        };
        // The export writes a library, which is what the tabs need.
        const settings = settingsOf(device.path);
        deviceSettings.set(device.path, {
          ...settings,
          hasDeviceLibrary: true,
          hasOneLibrary: true,
          hasLibrarySettings: true,
          deviceName: settings.deviceName || "REKORDBOX-LITE",
        });
      }
      return wait({
        tracks,
        playlists: 1,
        bytesCopied: (tracks - reused) * 8_000_000,
        analysisFiles: tracks - reused,
        reused,
        removed: 0,
        skipped: [],
        verified: true,
      });
    },

    // One device, so the panel has something to show. A browser cannot see a
    // real volume; the app asks the OS.
    listDevices: () => wait(devices.map((device) => ({ ...device }))),

    // A deck that keeps time but makes no sound. The audio engine is Rust and
    // is not here, so this counts frames and emits the same ticks the engine
    // does; everything above it — the scrolling waveform, the cue point, the
    // readouts — then behaves in a browser exactly as it does in the app, and
    // can be tested. What a browser cannot do is make a noise.
    deckLoad: (deck, trackId) => {
      if (deck !== "a") return wait(undefined);
      const index = Number.parseInt(trackId, 10) - 100000;
      const row = all[index];
      deckA.frames = 0;
      deckA.totalFrames = row ? row.durationSec * SAMPLE_RATE : 0;
      deckA.playing = false;
      deckA.loaded = row !== undefined;
      deckA.generation += 1;
      stopClock();
      for (const listener of deckEventListeners) {
        listener({
          deck: "a",
          totalFrames: deckA.totalFrames,
          sampleRate: SAMPLE_RATE,
          message: row ? null : "That track's file could not be found.",
        });
      }
      sendTick();
      return wait(undefined);
    },
    deckUnload: () => {
      deckA.loaded = false;
      deckA.playing = false;
      deckA.frames = 0;
      stopClock();
      sendTick();
      return wait(undefined);
    },
    deckPlay: () => {
      if (!deckA.loaded) return wait(undefined);
      deckA.playing = true;
      startClock();
      return wait(undefined);
    },
    deckPause: () => {
      deckA.playing = false;
      stopClock();
      sendTick();
      return wait(undefined);
    },
    deckSeek: (_deck, positionMs) => {
      deckA.frames = Math.max(0, Math.round((positionMs / 1000) * SAMPLE_RATE));
      deckA.generation += 1;
      sendTick();
      return wait(undefined);
    },
    // A browser has no audio, so a drag is a seek that follows the pointer:
    // the position moves, nothing is heard, and the visuals are the same.
    deckScrubBegin: () => wait(undefined),
    deckScrubTo: (_deck, positionMs) => {
      deckA.frames = Math.max(0, Math.round((positionMs / 1000) * SAMPLE_RATE));
      sendTick();
      return wait(undefined);
    },
    setMasterLevel: (level) => {
      master = Math.min(Math.max(level, 0), 1);
      sendTick();
      return wait(undefined);
    },
    // A browser has one output and no way to name it, so the list is empty
    // and the picker says so rather than inventing devices.
    audioDevices: () => wait({ devices: [], default: null, chosen: null }),
    setAudioDevice: () => wait(undefined),
    // The mixer is the engine's; a browser has no audio to apply it to, so
    // these are accepted and dropped rather than pretended at.
    // The tempo is the engine's, but the mock keeps it so the readout and the
    // MT button move: a browser has no audio to apply it to, and a control
    // that does not respond reads as a broken one.
    deckTempo: (deck, tempo) => {
      const on = deck === "b" ? deckB : deckA;
      on.tempo = Math.min(Math.max(tempo, 0.5), 2);
      sendTick();
      return wait(undefined);
    },
    deckMasterTempo: (deck, on) => {
      (deck === "b" ? deckB : deckA).masterTempo = on;
      sendTick();
      return wait(undefined);
    },
    setChannelBand: () => wait(undefined),
    setChannelKill: () => wait(undefined),
    setChannelTrim: () => wait(undefined),
    setCrossfade: () => wait(undefined),
    setEqCurve: () => wait(undefined),
    deckScrubEnd: () => {
      deckA.generation += 1;
      sendTick();
      return wait(undefined);
    },
    // A browser cannot see its own process. Zeroes would read as an app that
    // costs nothing, so every figure the platform will not give is null.
    appDiagnostics: () =>
      wait({ cpu: 0, memoryMb: 0, threads: null, openFiles: null, gpu: null }),
    // A browser has no Finder to open. Refusing is the truth; succeeding
    // silently made the menu item look as if it had done something.
    revealTrack: () =>
      wait(undefined).then(() => {
        throw new Error("A browser cannot show a file in the Finder.");
      }),

    deckState: () => wait(tick()),
    onDeckTick: (listener) => {
      deckTickListeners.add(listener);
      return () => deckTickListeners.delete(listener);
    },
    // A browser has no audio callback, so there is nothing to meter and no
    // beat to send it on.
    onMeters: () => () => undefined,
    onDeckEvent: (listener) => {
      deckEventListeners.add(listener);
      return () => deckEventListeners.delete(listener);
    },

    onLibraryReady: (listener) => {
      readyListeners.add(listener);
      return () => readyListeners.delete(listener);
    },
    onLibraryError: () => () => undefined,

    // A browser has no native menu bar. The mock exposes the listener so a
    // test can fire an item the way the shell would; this is the mock, which
    // exists to be driven, rather than a seam in the app.
    onMenu: (listener) => {
      const w = window as unknown as { __menu?: (id: string) => void };
      w.__menu = listener;
      return () => {
        delete w.__menu;
      };
    },

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

    // The device tabs. Held per stick so a change survives switching tabs
    // and devices, the way a written file would.
    deviceSettings: (path) => wait(copySettings(settingsOf(path))),
    saveDeviceSettings: (path, settings) => {
      const current = settingsOf(path);
      // A stick without a library keeps its reference rows: nothing to
      // write them into, as the real backend also refuses.
      const next: DeviceSettings = current.hasLibrarySettings
        ? { ...copySettings(settings), hasDevSetting: true }
        : {
            ...current,
            hasDevSetting: true,
            waveformColor: settings.waveformColor,
            waveformPosition: settings.waveformPosition,
            overviewWaveform: settings.overviewWaveform,
            keyDisplay: settings.keyDisplay,
          };
      deviceSettings.set(path, next);
      return wait(copySettings(next));
    },

    onLibraryChanged: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    filterValues: (spec) => {
      if (!ready) return notReady();
      // Over the source and query alone, never the filter's own result.
      const bpms = new Map<number, number>();
      const keys = new Map<string, number>();
      for (const i of candidatesFor(spec)) {
        const row = all[i];
        if (!row) continue;
        const whole = wholeBpm(row.bpmX100);
        if (whole > 0) bpms.set(whole, (bpms.get(whole) ?? 0) + 1);
        if (row.key !== "") keys.set(row.key, (keys.get(row.key) ?? 0) + 1);
      }
      const values: FilterValues = {
        bpms: [...bpms].map(([value, count]) => ({ value, count })).sort((x, y) => x.value - y.value),
        keys: [...keys]
          .map(([value, count]) => ({ value, count }))
          .sort((x, y) => {
            const [nx, lx, sx] = keyOrder(x.value);
            const [ny, ly, sy] = keyOrder(y.value);
            return nx - ny || lx - ly || sx.localeCompare(sy);
          }),
        // What the reference library holds, read once: one used category and
        // rekordbox's three unused ones, which it names `Empty Category`.
        tags: [
          {
            name: "Lexicon Tags",
            tags: [
              "Components ▶ Synth", "Components ▶ Vocal", "Components ▶ Beat",
              "Components ▶ Sub Bass", "Components ▶ Percussion", "Components ▶ Piano",
              "Components ▶ Upper", "Situation ▶ Main Floor", "Situation ▶ Second Floor",
              "Situation ▶ Lounge",
            ],
          },
          { name: "Empty Category", tags: [] },
          { name: "Empty Category", tags: [] },
          { name: "Empty Category", tags: [] },
        ],
      };
      return wait(values);
    },

    onCuesChanged: (listener) => {
      cueListeners.add(listener);
      return () => cueListeners.delete(listener);
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

/**
 * Simulated IPC latency, from `?latency=25`.
 *
 * The real backend answers `fetch_rows` in single-digit milliseconds and the
 * mock answers in zero, and the difference is not cosmetic: a race between a
 * moving window and a landing page cannot happen at zero. This is how a scroll
 * is tested against a backend that takes any time at all.
 */
/** Whether the mock's library reports itself writable, from `?writable=1`. */
function readWritableFromUrl(): boolean {
  if (typeof location === "undefined") return false;
  return new URLSearchParams(location.search).get("writable") === "1";
}

function readLatencyFromUrl(): number | null {
  if (typeof location === "undefined") return null;
  const raw = new URLSearchParams(location.search).get("latency");
  const n = raw ? Number.parseInt(raw, 10) : NaN;
  return Number.isFinite(n) && n >= 0 && n <= 2000 ? n : null;
}
