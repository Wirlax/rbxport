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
  AppErrorDto, Backend, Cue, DeckEvent, Device, DeviceSettings, Edits, ExplorerRoot,
  FilterValues, LibrarySummary, Limiter, LinkPeerSeen, LinkStatus, RowDto, SortKey, Tick, TrackDetails, TrackField,
  PreferencesRequest, UpdateCheck, UpdateProgress,
  TrackFilter, TreeNode, ViewHandle, ViewSpec, WaveformKind,
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
 * The two hot-cue sets the reference playlist carries, in the colours
 * rekordbox draws them — the default green on A–D, and the E–H set one DJ
 * tool writes in teal, orange, blue and yellow — plus the coloured A–D set,
 * so every measured colour is on screen somewhere.
 */
const CUE_SETS: readonly (readonly (readonly [string, number, string | null])[])[] = [
  [["A", 0.12, "#77E866"], ["B", 0.34, "#77E866"], ["C", 0.61, "#77E866"], ["D", 0.83, "#77E866"]],
  [["E", 0.12, "#51AE7B"], ["F", 0.29, "#F09235"], ["G", 0.46, "#3A59F6"], ["H", 0.70, "#D9AC3A"]],
  [["A", 0.12, "#E13A8A"], ["B", 0.26, "#6AAEEC"], ["C", 0.61, "#A8D54B"], ["D", 0.79, "#A274F7"]],
];

/** What a hot cue added here draws: index 21, the default the writer stores. */
const DEFAULT_CUE_COLOUR = "#77E866";

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
    // The draws stay in this order: the e2e suite picks rows by index and
    // relies on which of them the seed makes analysed.
    const title = `${TITLES[Math.floor(rnd() * TITLES.length)]} ${MIXES[Math.floor(rnd() * MIXES.length)]}`;
    const album = rnd() > 0.6 ? "Single" : "";
    const genre = GENRES[Math.floor(rnd() * GENRES.length)] ?? "";
    const label = LABELS[Math.floor(rnd() * LABELS.length)] ?? "";
    const comment = analysed && rnd() > 0.5 ? `${1 + Math.floor(rnd() * 12)}A - ${key.slice(0, 1)} - ${bpmX100 / 100}` : "";
    const durationSec = 180 + Math.floor(rnd() * 240);
    const rating = Math.floor(rnd() * 6);
    const dateAdded = `2026-0${1 + Math.floor(rnd() * 9)}-${String(day).padStart(2, "0")}`;
    const releaseDate = rnd() > 0.3 ? `2026-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}` : "";
    const cueSet = analysed ? (CUE_SETS[Math.floor(rnd() * CUE_SETS.length)] ?? []) : [];
    rows[i] = {
      id: String(100000 + i),
      trackNo: i + 1,
      title,
      artist,
      album,
      genre,
      label,
      comment,
      bpmX100,
      key,
      durationSec,
      rating,
      analysed,
      dateAdded,
      releaseDate,
      hotCues: cueSet.map(([letter, at, colour]) => [letter, Math.round(durationSec * 1000 * at), colour]),
      artworkHue: Math.floor(rnd() * 360),
      // The mock has no files to serve, so every row falls back to the tint.
      hasArtwork: false,
      fileName: `${String(i + 1).padStart(2, "0")} ${artist} - track.mp3`,
    };
  }
  return rows;
}

/**
 * A fake disk for the Explorer.
 *
 * The shape of the capture: the music and home folders, the system volume,
 * one stick. The music folder's Downloads holds twelve files, the first six
 * of them library rows — so a folder view shows both kinds — and Sets holds
 * three loose files. Everything else is folders, and a folder not listed here
 * has nothing under it, which is what the real backend answers for a folder
 * it cannot read.
 */
const EXPLORER_ROOTS: readonly ExplorerRoot[] = [
  { name: "Music", path: "/Users/mock/Music" },
  { name: "mock", path: "/Users/mock" },
  { name: "Macintosh HD", path: "/" },
  { name: "SD", path: "/Volumes/SD" },
];
const EXPLORER_CHILDREN: ReadonlyMap<string, readonly string[]> = new Map([
  ["/Users/mock/Music", ["Downloads", "Rekordbox", "Sets"]],
  ["/Users/mock", ["Desktop", "Documents", "Music"]],
  ["/", ["Applications", "Library", "System", "Users"]],
  ["/Users", ["mock", "Shared"]],
  ["/Volumes/SD", ["Contents", "PIONEER"]],
  ["/Volumes/SD/PIONEER", ["rekordbox", "USBANLZ"]],
]);
/** Files per folder: a library row index, or a loose file's name. */
const EXPLORER_FILES: ReadonlyMap<string, readonly (number | string)[]> = new Map([
  ["/Users/mock/Music/Downloads", [0, 1, 2, 3, 4, 5, "Untitled Bounce 1.wav", "Untitled Bounce 2.wav", "demo_128.aiff", "live edit.mp3", "promo (radio).mp3", "voice memo.m4a"]],
  ["/Users/mock/Music/Sets", ["Set 2026-08-30.mp3", "Set 2026-09-04.mp3", "Warmup.flac"]],
]);

/** A row for a file the library does not hold: its name, and nothing else. */
function looseRow(folder: string, name: string, position: number): RowDto {
  return {
    id: `file:${folder}/${name}`,
    trackNo: position,
    title: name.replace(/\.[^.]+$/, ""),
    artist: "",
    album: "",
    genre: "",
    label: "",
    comment: "",
    bpmX100: 0,
    key: "",
    durationSec: 0,
    rating: 0,
    analysed: 0,
    dateAdded: "",
    releaseDate: "",
    hotCues: [],
    artworkHue: 0,
    hasArtwork: false,
    fileName: name,
  };
}

const FOLDERS = ["CURRENT", "USB", "DOWNLOADS"];
const PLAYLISTS = [
  "Melodic Vox", "Hardstyle", "Drum and Bass", "Eurodance", "Latin", "Main", "Main: Vocal",
  "Melodic Techno", "Techno", "Trance", "Groovy", "Fun House", "House", "Tech House",
  "Special", "HOUSE CLASSIC", "TRIODE",
];

/** How many tracks a mock playlist holds, from its id, so every path agrees. */
function mockPlaylistSize(id: string): number {
  const seed = [...id].reduce((a, c) => a + c.charCodeAt(0), 0);
  return 14 + (seed % 30);
}

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
      const id = `pl-${fi}-${i}`;
      // The count the real tree carries, from the same seed `openView` uses.
      nodes.push({ id, name, kind: "playlist", depth: 2, childCount: mockPlaylistSize(id) });
    }
    // One intelligent playlist in the first folder, as a library with rules
    // shows: no count, since a rule's count is only known once it is opened.
    if (fi === 0) {
      nodes.push({ id: "smart-0", name: "Fresh 128s", kind: "smartPlaylist", depth: 2 });
    }
  }
  // Histories, filed as rekordbox files them: a folder per year, one per month
  // inside it under the month's name, and the sessions under that. The year
  // open and the month closed, as the shell sends them.
  nodes.push({ id: "histories", name: "Histories", kind: "histories", depth: 0, expanded: true });
  nodes.push({ id: "hist-2026", name: "2026", kind: "history", depth: 1, expanded: true });
  nodes.push({ id: "hist-202609", name: "September", kind: "history", depth: 2, expanded: false });
  for (const day of ["2026-09-04", "2026-08-30", "2026-08-23"]) {
    nodes.push({ id: `hist-${day}`, name: `LINK HISTORY ${day}`, kind: "history", depth: 3 });
  }
  return nodes;
}

const collator = new Intl.Collator("en", { sensitivity: "base", numeric: true });

function compare(a: RowDto, b: RowDto, col: SortKey): number {
  switch (col) {
    case "keyCamelot": {
      // Round the wheel, unknown keys last, as the index ranks them.
      const rank = (key: string) => {
        const code = toCamelot(key);
        if (code === "") return Number.MAX_SAFE_INTEGER;
        return Number(code.slice(0, -1)) * 2 + (code.endsWith("B") ? 1 : 0);
      };
      return rank(a.key) - rank(b.key) || collator.compare(a.key, b.key);
    }
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
  const writable = options.writable ?? readFlagFromUrl("writable");
  const all = makeRows(trackCount);
  const colors = makeColors(trackCount);
  // What the row DTO does not carry, made up per track and edited in place.
  const details = new Map<string, TrackDetails>();
  const folded = all.map((r) => fold(`${r.title} ${r.artist} ${r.album} ${r.comment}`));

  /** The rows a source and query leave, before sorting and before the filter. */
  // The mock owns the tree and the playlists' contents the same way Rust
  // does, so the edit flows can be driven end to end in `pnpm dev:mock` and
  // in Playwright without a database.
  let generation = 1;
  /**
   * What a playlist holds once anything has been done to it, as track ids in
   * playing order. A playlist nobody has touched is not here and shows its
   * seeded slice; one made here starts empty, as a new one does.
   */
  const membership = new Map<string, string[]>();
  let nextId = 1;
  const indexOfId = new Map(all.map((row, i) => [row.id, i] as const));

  /** The deterministic slice a list shows before it is edited. */
  const seededMembers = (id: string): number[] => {
    const seed = [...id].reduce((a, c) => a + c.charCodeAt(0), 0);
    const size = 14 + (seed % 30);
    return Array.from({ length: size }, (_, i) => (seed * 37 + i * 101) % trackCount);
  };
  /** A playlist's tracks as ids, seeding the membership on first edit. */
  const membersOf = (playlist: string): string[] => {
    const held = membership.get(playlist);
    if (held) return held;
    const seeded = seededMembers(playlist).map((i) => all[i]?.id ?? "");
    membership.set(playlist, seeded);
    return seeded;
  };
  /** The count the tree and the export report carry. */
  const playlistSize = (id: string): number =>
    membership.get(id)?.length ?? mockPlaylistSize(id);

  const candidatesFor = (spec: ViewSpec): number[] => {
    let candidates: number[];
    if (spec.source.kind === "playlist") {
      const held = membership.get(spec.source.id);
      candidates = held
        ? held.map((id) => indexOfId.get(id)).filter((i): i is number => i !== undefined)
        : seededMembers(spec.source.id);
    } else if (spec.source.kind === "history") {
      candidates = seededMembers(spec.source.id);
    } else {
      candidates = Array.from({ length: trackCount }, (_, i) => i);
    }
    const q = fold(spec.query.trim());
    if (q) candidates = candidates.filter((i) => (folded[i] ?? "").includes(q));
    return candidates;
  };
  const tree = makeTree();

  const views = new Map<number, { order: Uint32Array; gen: number }>();
  // A folder's rows, held whole: a folder is a few files here and a few
  // thousand at most on a disk, which is why the real backend keeps the list
  // and pages it out rather than sending it.
  const folderViews = new Map<number, RowDto[]>();
  let nextViewId = 1;

  const listeners = new Set<(generation: number) => void>();

  /**
   * Every edit bumps the generation and tells anyone listening, exactly as a
   * real write does — the backend reloads and emits `library:changed`.
   */
  const bump = (): Promise<number> => {
    generation += 1;
    // The counts the tree shows follow the edit, as the re-read tree does.
    for (const node of tree) {
      if (node.kind === "playlist") node.childCount = playlistSize(node.id);
    }
    for (const listener of listeners) listener(generation);
    return wait(generation);
  };

  const findNode = (id: string) => tree.find((n) => n.id === id);

  /**
   * Puts a new node where the re-read tree would show it: last under its
   * parent, or last among the top-level playlists — before the Histories,
   * whose collapsed heading would otherwise hide anything appended after it.
   */
  const insertUnder = (parent: string, node: TreeNode) => {
    let at: number;
    if (parent === TREE_ROOT) {
      at = tree.findIndex((n) => n.depth === 0 && n.id !== "all" && n.id !== "playlists");
    } else {
      const start = tree.findIndex((n) => n.id === parent);
      const depth = tree[start]?.depth ?? 0;
      at = start + 1;
      while (at < tree.length && (tree[at]?.depth ?? 0) > depth) at += 1;
    }
    tree.splice(at < 0 ? tree.length : at, 0, node);
  };

  /**
   * A parent's children, as the flat tree encodes them.
   *
   * There is no parent field here: depth and array order are the structure, so
   * a child is a node one level down before the run returns to the parent's
   * own level.
   */
  const childrenOf = (parent: string): TreeNode[] => {
    const start = parent === TREE_ROOT ? -1 : tree.findIndex((n) => n.id === parent);
    if (parent !== TREE_ROOT && start < 0) return [];
    const depth = parent === TREE_ROOT ? 0 : (tree[start]?.depth ?? 0);
    const out: TreeNode[] = [];
    for (let i = start + 1; i < tree.length; i += 1) {
      const node = tree[i];
      if (!node) continue;
      if (parent !== TREE_ROOT && node.depth <= depth) break;
      if (node.depth === depth + 1) out.push(node);
    }
    return out;
  };

  /** How many nodes a node spans: itself and everything under it. */
  const subtreeLength = (at: number): number => {
    const depth = tree[at]?.depth ?? 0;
    let end = at + 1;
    while (end < tree.length && (tree[end]?.depth ?? 0) > depth) end += 1;
    return end - at;
  };

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

  const edits: Edits = {
    createPlaylist: (name, parent) => {
      const depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      const id = `made-${nextId++}`;
      membership.set(id, []);
      insertUnder(parent, { id, name, kind: "playlist", depth, childCount: 0 });
      return bump();
    },
    createFolder: (name, parent) => {
      const depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      insertUnder(parent, { id: `made-${nextId++}`, name, kind: "folder", depth, expanded: true });
      return bump();
    },
    renamePlaylist: (id, name) => {
      const node = findNode(id);
      if (node) node.name = name;
      return bump();
    },
    movePlaylist: (id, parent, index) => {
      const from = tree.findIndex((n) => n.id === id);
      if (from < 0) return bump();
      // A folder cannot be put inside itself: the subtree would be detached
      // from the tree and never seen again. The backend refuses it, so does this.
      const span = subtreeLength(from);
      const moving = tree.slice(from, from + span);
      if (moving.some((n) => n.id === parent)) {
        return Promise.reject(new Error("that would put a folder inside itself"));
      }

      // Where it is going, decided before the tree is disturbed.
      const siblings = childrenOf(parent).filter((n) => n.id !== id);
      const at = Math.min(index ?? siblings.length, siblings.length);
      const after = siblings[at];

      tree.splice(from, span);
      // Its new level, carried down through everything under it.
      const depth = parent === TREE_ROOT ? 1 : (findNode(parent)?.depth ?? 0) + 1;
      const shift = depth - (moving[0]?.depth ?? depth);
      for (const node of moving) node.depth += shift;

      let to: number;
      if (after) {
        to = tree.findIndex((n) => n.id === after.id);
      } else if (parent === TREE_ROOT) {
        const histories = tree.findIndex(
          (n) => n.depth === 0 && n.id !== "all" && n.id !== "playlists",
        );
        to = histories < 0 ? tree.length : histories;
      } else {
        const start = tree.findIndex((n) => n.id === parent);
        to = start < 0 ? tree.length : start + subtreeLength(start);
      }
      tree.splice(to < 0 ? tree.length : to, 0, ...moving);
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
      const current = membersOf(playlist);
      for (const track of tracks) if (!current.includes(track)) current.push(track);
      membership.set(playlist, current);
      return bump();
    },
    removeTracksFromPlaylist: (playlist, tracks) => {
      const current = membersOf(playlist).filter((t) => !tracks.includes(t));
      membership.set(playlist, current);
      return bump();
    },
    resetPlayCount: () => bump(),
    // The mock keeps no history sessions of its own to add to or take from.
    recordPlay: () => wait(generation),
    removeFromHistory: () => bump(),
    // The mock's rows are addressed by index, so a removal only takes the
    // tracks out of every playlist; the collection keeps its count.
    removeFromCollection: (tracks) => {
      for (const [playlist, members] of membership) {
        membership.set(playlist, members.filter((t) => !tracks.includes(t)));
      }
      return bump();
    },
    reorderPlaylist: (playlist, tracks) => {
      const current = membersOf(playlist);
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
      const d = details.get(track);
      if (d) d.color = color ?? "0";
      return bump();
    },
    addArtwork: (track) => {
      const row = all.find((r) => r.id === track);
      if (row) {
        row.hasArtwork = true;
        const d = details.get(track);
        if (d) d.hasArtwork = true;
      }
      return bump();
    },
    clearArtwork: (track) => {
      const row = all.find((r) => r.id === track);
      if (row) {
        row.hasArtwork = false;
        const d = details.get(track);
        if (d) d.hasArtwork = false;
      }
      return bump();
    },
    setTrackField: (track, field, value) => {
      const row = all.find((r) => r.id === track);
      if (!row) return bump();
      const d = detailsOf(row);
      // The same refusals the writer makes: a number that is not one, and a
      // key the library does not hold.
      const numeric: Partial<Record<TrackField, "year" | "trackNumber" | "discNumber" | "playCount">> = {
        year: "year", trackNumber: "trackNumber", discNumber: "discNumber", playCount: "playCount",
      };
      const which = numeric[field];
      if (which) {
        const n = /^\s*\d+\s*$/.test(value) ? Number.parseInt(value, 10) : NaN;
        if (!Number.isFinite(n)) {
          return Promise.reject(new Error(`${JSON.stringify(value)} is not a whole number`));
        }
        d[which] = n;
        return bump();
      }
      if (field === "key" && value !== "" && !KEYS.includes(value)) {
        return Promise.reject(new Error(`${JSON.stringify(value)} is not a key the library knows`));
      }
      if (field === "bpm") {
        const bpm = Number.parseFloat(value.trim());
        if (!Number.isFinite(bpm) || bpm < 20 || bpm > 400) {
          return Promise.reject(new Error(`${JSON.stringify(value)} is not a BPM between 20 and 400`));
        }
        row.bpmX100 = Math.round(bpm * 100);
        d.bpmX100 = row.bpmX100;
        return bump();
      }
      // Narrowed by hand: what is left after the numeric fields is text.
      const text = field as Exclude<TrackField, "year" | "trackNumber" | "discNumber" | "playCount" | "bpm">;
      d[text] = value.trim();
      // The row carries some of the same columns; keep the two in step the
      // way a reload of the index would.
      if (field === "title") row.title = d.title;
      else if (field === "artist") row.artist = d.artist;
      else if (field === "album") row.album = d.album;
      else if (field === "genre") row.genre = d.genre;
      else if (field === "label") row.label = d.label;
      else if (field === "key") row.key = d.key;
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
        colour: kind === "memory" ? null : DEFAULT_CUE_COLOUR,
      });
      return cuesChanged(track, id);
    },
    addLoop: (track, kind, inMs, outMs) => {
      if (!all.some((r) => r.id === track)) return refuse(`no track ${track}`);
      if (outMs <= inMs) return refuse("a loop has to end after it starts");
      const id = `cue-${nextCueId++}`;
      cuesOf(track).push({
        id, positionMs: inMs, outMs, letter: kind === "memory" ? "" : kind.hot, memory: kind === "memory",
        colour: kind === "memory" ? null : DEFAULT_CUE_COLOUR,
      });
      return cuesChanged(track, id);
    },
    convertMemoryCuesToHot: (track) => {
      const cues = cuesOf(track);
      const taken = new Set(cues.map((c) => c.letter));
      const free = [..."ABCDEFGHIJKLMNOP"].filter((letter) => !taken.has(letter));
      const memory = cues.filter((c) => c.memory).sort((a, b) => a.positionMs - b.positionMs);
      let made = 0;
      for (const cue of memory) {
        const letter = free.shift();
        if (letter === undefined) break;
        cues.push({
          id: `cue-${nextCueId++}`, positionMs: cue.positionMs, outMs: cue.outMs, letter, memory: false,
          colour: DEFAULT_CUE_COLOUR,
        });
        made += 1;
      }
      return cuesChanged(track, made);
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

  /**
   * The rest of a track's record, invented once per track from its row.
   *
   * Deterministic so a test can name what it expects: the size follows the
   * duration at a constant bitrate, the sample rate is the common one, and
   * the path is where the mock says its files live.
   */
  const detailsOf = (row: RowDto): TrackDetails => {
    let d = details.get(row.id);
    if (d) return d;
    const seed = Number.parseInt(row.id, 10);
    const wav = seed % 7 === 0;
    const bitrate = wav ? 1411 : 320;
    d = {
      id: row.id,
      title: row.title,
      artist: row.artist,
      album: row.album,
      albumArtist: row.album ? row.artist : "",
      originalArtist: "",
      composer: seed % 5 === 0 ? row.artist : "",
      remixer: row.title.includes("Remix") ? "Someone" : "",
      lyricist: "",
      genre: row.genre,
      label: row.label,
      key: row.key,
      comment: row.comment,
      mixName: "",
      message: "",
      // The colour the filter bar matches this row by, so the deck's INFO
      // tab and the bar agree.
      color: String(colors[all.indexOf(row)] ?? 0),
      rating: row.rating,
      bpmX100: row.bpmX100,
      durationSec: row.durationSec,
      year: row.releaseDate ? Number.parseInt(row.releaseDate.slice(0, 4), 10) : 0,
      trackNumber: seed % 12,
      discNumber: 0,
      playCount: seed % 9,
      fileType: wav ? 11 : 1,
      fileSize: Math.round((row.durationSec * bitrate * 1000) / 8),
      bitrate,
      sampleRate: 44_100,
      bitDepth: wav ? 16 : 0,
      dateCreated: row.dateAdded.slice(0, 10),
      releaseDate: row.releaseDate,
      path: `/Volumes/MUSIC/${row.artist || "Unknown Artist"}/${row.title}.${wav ? "wav" : "mp3"}`,
      hotCueAutoLoad: true,
      publish: false,
      hasArtwork: false,
    };
    details.set(row.id, d);
    return d;
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
    // A memory cue at 2% and the row's own hot cues from 12% on, so the
    // detail's opening window holds exactly the one marker and the badges on
    // the row are the cues the deck shows.
    const made: Cue[] = [];
    if (row && row.analysed !== 0) {
      const total = row.durationSec * 1000;
      made.push(
        { id: `cue-${nextCueId++}`, positionMs: Math.round(total * 0.02), outMs: 0, letter: "", memory: true, colour: null },
        ...row.hotCues.map(([letter, positionMs, colour]) => (
          { id: `cue-${nextCueId++}`, positionMs, outMs: 0, letter, memory: false, colour }
        )),
      );
    }
    cueStore.set(trackId, made);
    return made;
  };
  /** The row's badges follow its cues, in slot order as the backend sends them. */
  const syncRow = (trackId: string) => {
    const row = all[Number.parseInt(trackId, 10) - 100000];
    if (!row) return;
    row.hotCues = cuesOf(trackId)
      .filter((cue) => !cue.memory)
      .sort((a, b) => a.letter.localeCompare(b.letter))
      .map((cue) => [cue.letter, cue.positionMs, cue.colour]);
  };
  const findCue = (id: string): { track: string; cue: Cue } | null => {
    for (const [track, list] of cueStore) {
      const cue = list.find((c) => c.id === id);
      if (cue) return { track, cue };
    }
    return null;
  };
  const cuesChanged = <T>(track: string, value: T): Promise<T> => {
    syncRow(track);
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
    tempo: 1, masterTempo: false, keyShift: 0, startInFrames: 0,
    loopInFrames: 0, loopOutFrames: 0, looping: false,
  };
  // Deck B holds its own tempo and key lock even though a browser has no
  // audio to apply them to: a control that snapped back on the next tick would
  // read as a broken one, and the deck it stands for does keep them.
  const deckB = {
    frames: 0, totalFrames: 0, generation: 0, playing: false, loaded: false,
    tempo: 1, masterTempo: false, keyShift: 0, startInFrames: 0,
    loopInFrames: 0, loopOutFrames: 0, looping: false,
  };
  const deckTickListeners = new Set<(tick: Tick) => void>();
  const deckEventListeners = new Set<(event: DeckEvent) => void>();
  let clock: ReturnType<typeof setTimeout> | null = null;
  let clockAt = 0;

  /** The master level, which a browser can hold even with nothing to apply it to. */
  let master = 1;

  /** The master limiter, likewise. */
  let limiter: Limiter = { enabled: true, ceilingDb: -0.3, releaseMs: 100 };

  const preferencesRequestListeners = new Set<(what: PreferencesRequest) => void>();

  /** Who is told how the pretend download is going. */
  const updateProgressListeners = new Set<(progress: UpdateProgress) => void>();

  const tick = (): Tick => ({
    a: { ...deckA },
    b: { ...deckB },
    sampleRate: SAMPLE_RATE,
    // A browser has no audio callback, so there is nothing to meter. Zero is
    // the truth here rather than a placeholder: nothing is coming out.
    peakLeft: 0,
    peakRight: 0,
    master,
    reduction: 0,
    // The mock stands in for the build that ships, which carries Rubber Band.
    shiftsKey: true,
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
      // Inside a loop the head rounds at the out point, as the deck does.
      if (deckA.looping && deckA.loopOutFrames > deckA.loopInFrames && deckA.frames >= deckA.loopOutFrames) {
        deckA.frames = deckA.loopInFrames + ((deckA.frames - deckA.loopOutFrames) % (deckA.loopOutFrames - deckA.loopInFrames));
      }
      clockAt = now;
      if (deckA.frames >= deckA.totalFrames) deckA.playing = false;
      sendTick();
      if (deckA.playing) clock = setTimeout(step, TICK_MS);
    };
    clock = setTimeout(step, TICK_MS);
  };

  const wait = <T>(value: T): Promise<T> =>
    latency > 0 ? new Promise((r) => setTimeout(() => r(value), latency)) : Promise.resolve(value);

  /** LINK in a browser: off, with nothing to run it on. */
  // The mock's tempo-master state, mutable so the master controls do
  // something in the browser.
  const mockMaster = { on: false, bpm: 120 };
  const linkOff = (): LinkStatus => ({
    on: false,
    problem: null,
    interface: null,
    players: [],
    interfaces: [],
    master: false,
    masterBpm: mockMaster.bpm,
  });

  /** A network to look at, from `?link=`; null in a plain browser. */
  const linkMode = readLinkFromUrl();
  const mockPeers: LinkPeerSeen[] = [
    { number: 1, name: "CDJ-3000", kind: "player", address: "192.168.1.152" },
    { number: 2, name: "CDJ-3000", kind: "player", address: "192.168.1.153" },
    { number: 33, name: "DJM-V5", kind: "mixer", address: "192.168.1.155" },
  ];
  const mockLinkOn = (): LinkStatus => ({
    on: true,
    problem: null,
    interface: { name: "en0", address: "192.168.1.14" },
    interfaces: [{ name: "en0", address: "192.168.1.14" }],
    players: [
      {
        number: 1,
        name: "CDJ-3000",
        kind: "player",
        address: "192.168.1.152",
        loaded: { id: "1", title: "GIN AND TONIC (Extended Mix)", artist: "DONT BLINK" },
        playing: true,
        master: true,
      },
      { number: 2, name: "CDJ-3000", kind: "player", address: "192.168.1.153", loaded: null, playing: false, master: false },
      { number: 33, name: "DJM-V5", kind: "mixer", address: "192.168.1.155", loaded: null, playing: false, master: false },
    ],
    master: mockMaster.on,
    masterBpm: mockMaster.bpm,
  });
  const mockLinkStatus = (): LinkStatus => {
    if (linkMode === "on") return mockLinkOn();
    if (linkMode === "blocked") {
      return { ...linkOff(), problem: "rekordbox is running and holds the link ports. Quit it to turn LINK on." };
    }
    return linkOff();
  };

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
      if (spec.source.kind === "folder") {
        const folder = spec.source.path;
        const q = fold(spec.query.trim());
        const rows = (EXPLORER_FILES.get(folder) ?? [])
          .map((entry, at) =>
            typeof entry === "number"
              ? { ...(all[entry] ?? looseRow(folder, "", at + 1)), trackNo: at + 1 }
              : looseRow(folder, entry, at + 1),
          )
          .filter((row) => q === "" || fold(`${row.title} ${row.artist} ${row.fileName ?? ""}`).includes(q));
        if (spec.sort !== "trackNo") {
          rows.sort((x, y) => {
            const c = compare(x, y, spec.sort);
            return spec.descending ? -c : c;
          });
        } else if (spec.descending) {
          rows.reverse();
        }
        const viewId = nextViewId++;
        folderViews.set(viewId, rows);
        return wait<ViewHandle>({ viewId, len: rows.length, gen: 1 });
      }
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
      // `trackNo` is not a column to rank by: it means "leave them in the
      // order this view produced them", which for a playlist is its
      // membership — the order somebody dragged them into. Ranking by the
      // row's own stored number put the collection's order back instead, so
      // a reordered playlist came out looking untouched.
      const sorted =
        spec.sort === "trackNo"
          ? (spec.descending ? Array.from(order).reverse() : Array.from(order))
          : Array.from(order).sort((x, y) => {
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
      const folder = folderViews.get(viewId);
      if (folder) return wait(folder.slice(Math.max(0, offset), Math.max(0, offset) + len));
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
      // The overview tags are 1,200 columns for any track; the detail ones
      // are far denser.
      const detail = kind === "bandsDetail" || kind === "monoDetail" || kind === "colourDetail";
      const columns = detail ? 12_000 : 1200;
      // Each palette's tag in its own layout, from the same three bands, so
      // a palette switch shows the same track drawn another way.
      const stride = kind === "mono" || kind === "monoDetail" ? 1
        : kind === "colour" ? 6
          : kind === "colourDetail" ? 2
            : 3;
      const out = new Uint8Array(columns * stride);
      for (let i = 0; i < columns; i++) {
        const at = i / columns;
        // A shape with quiet intros and outros, so the overview reads as a
        // track rather than a block.
        const envelope = Math.min(1, Math.min(at, 1 - at) * 6) * (0.55 + 0.45 * Math.abs(Math.sin(at * Math.PI * 5)));
        const jitter = 0.75 + rnd() * 0.25;
        const low = Math.round(envelope * jitter * 110);
        const mid = Math.round(envelope * jitter * 70);
        // Highs are sparse, which is what puts the bright core in the middle.
        const high = Math.round(envelope * (rnd() > 0.7 ? rnd() * 45 : rnd() * 8));
        const height = Math.max(low, mid, high);
        const base = i * stride;
        if (stride === 3) {
          out[base] = low;
          out[base + 1] = mid;
          out[base + 2] = high;
        } else if (stride === 1) {
          // Five bits of height, three of whiteness from how much is highs.
          out[base] = (Math.round((height / 127) * 31) & 0x1f) | (Math.min(7, Math.round((high / 45) * 7)) << 5);
        } else if (stride === 6) {
          // Height, then red green blue as mid high low.
          out[base] = height;
          out[base + 3] = mid;
          out[base + 4] = high;
          out[base + 5] = low;
        } else {
          const word = (Math.min(7, mid >> 4) << 13) | (Math.min(7, high >> 3) << 10) | (Math.min(7, low >> 4) << 7)
            | ((Math.round((height / 127) * 31) & 0x1f) << 2);
          out[base] = word >> 8;
          out[base + 1] = word & 0xff;
        }
      }
      if (!window) return wait(out);
      const first = Math.min(window.from, columns) * stride;
      const len = Math.min(window.len, columns - window.from) * stride;
      return wait(out.subarray(first, first + Math.max(0, len)));
    },

    viewIdsInRange: (viewId, from, to) => {
      const [lo, hi] = from <= to ? [from, to] : [to, from];
      const folder = folderViews.get(viewId);
      if (folder) return wait(folder.slice(Math.max(0, lo), hi + 1).map((row) => row.id));
      const view = views.get(viewId);
      if (!view) return Promise.reject(new Error(`unknown view ${viewId}`));
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

    // The memory cue and the row's own hot cues, so the player's markers and
    // list agree with the badges on the row that was loaded. A copy, ordered
    // by position as the index orders them, so an edit cannot reach the store.
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
    exportPlaylist: (playlistId, destination, defaults) => {
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
        // The export writes a library, which is what the tabs need. A stick
        // that had none takes the Preferences window's defaults, as the
        // real export does; one that had its own keeps them.
        const settings = settingsOf(device.path);
        const fresh = !settings.hasLibrarySettings && defaults !== undefined;
        deviceSettings.set(device.path, {
          ...settings,
          ...(fresh
            ? {
                hasDevSetting: true,
                waveformColor: defaults.waveformColor,
                waveformPosition: defaults.waveformPosition,
                overviewWaveform: defaults.overviewWaveform,
                keyDisplay: defaults.keyDisplay,
                categories: defaults.categories ?? settings.categories,
                sorts: defaults.sorts ?? settings.sorts,
                subColumn: defaults.subColumn,
              }
            : {}),
          hasDeviceLibrary: true,
          hasOneLibrary: true,
          hasLibrarySettings: true,
          deviceName: settings.deviceName || "RBXPORT",
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
    onExportProgress: () => () => undefined,
    // Two backups, as a session that has edited twice would have.
    listBackups: () =>
      wait([
        { path: "/mock/backups/master-2026-09-17-09-12-04-000--00-00.db", name: "master-2026-09-17-09-12-04-000--00-00.db", bytes: 41_943_040 },
        { path: "/mock/backups/master-2026-09-16-18-40-51-000--00-00.db", name: "master-2026-09-16-18-40-51-000--00-00.db", bytes: 41_811_968 },
      ]),
    backUpLibrary: () => wait("/mock/backups/master-2026-09-17-21-00-00-000--00-00.db"),
    restoreBackup: () => bump(),
    // A browser cannot ask; the answer is yes, so the flow can be driven.
    confirm: () => Promise.resolve(true),

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
    // The wait is a timer here rather than counted in output frames: a
    // browser has no callback to count them in, and the timing is only
    // ever judged by ear against a real device.
    deckPlayAfter: (_deck, delayMs) => {
      if (!deckA.loaded) return wait(undefined);
      deckA.playing = true;
      setTimeout(startClock, Math.max(0, delayMs));
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
    deckSetLoop: (_deck, inMs, outMs) => {
      const from = Math.max(0, Math.round((inMs / 1000) * SAMPLE_RATE));
      const to = Math.max(0, Math.round((outMs / 1000) * SAMPLE_RATE));
      if (to <= from) return wait(undefined);
      deckA.loopInFrames = from;
      deckA.loopOutFrames = to;
      deckA.looping = true;
      if (deckA.frames >= to || deckA.frames < from) {
        deckA.frames = from;
        deckA.generation += 1;
      }
      sendTick();
      return wait(undefined);
    },
    deckLoopActive: (_deck, on) => {
      if (deckA.loopOutFrames <= deckA.loopInFrames) return wait(undefined);
      deckA.looping = on;
      if (on) {
        deckA.frames = deckA.loopInFrames;
        deckA.generation += 1;
      }
      sendTick();
      return wait(undefined);
    },
    deckClearLoop: () => {
      deckA.loopInFrames = 0;
      deckA.loopOutFrames = 0;
      deckA.looping = false;
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
    // Held and given back clamped as the engine would, so the controls in
    // Settings behave in a browser.
    // A browser has nothing to update, so this offers a pretend version two
    // releases on, with a changelog shaped like the real one, and its
    // download runs at a believable pace so the bar can be watched.
    checkForUpdate: () =>
      wait<UpdateCheck>({
        currentVersion: "0.4.0",
        version: "0.6.0",
        date: "2026-09-12T18:00:00Z",
        changes: [
          {
            version: "0.6.0",
            date: "2026-09-12",
            body:
              "## [0.6.0] — 2026-09-12\n\n### Added\n- The app checks for a newer version when it " +
              "starts, downloads it with a progress bar, and shows what changed since the version " +
              "running.\n\n### Fixed\n- A track dragged to a player carries a faded copy of its row.",
          },
          {
            version: "0.5.0",
            date: "2026-09-11",
            body:
              "## [0.5.0] — 2026-09-11\n\n### Added\n- The app icon is the rekordbox cube ring with a " +
              "feather in the middle.\n\n### Changed\n- The right-click menus list what rekordbox's do.",
          },
        ],
      }),
    installUpdate: () =>
      new Promise<void>((_resolve, reject) => {
        const total = 16_342_693;
        let downloaded = 0;
        const tick = () => {
          downloaded = Math.min(total, downloaded + 900_000 + Math.random() * 400_000);
          for (const listener of updateProgressListeners) listener({ downloaded, total });
          if (downloaded < total) {
            setTimeout(tick, 100);
          } else {
            // A real install restarts the app; a browser cannot, so the
            // manager is told what it would be told if the install failed
            // after the download — which is the only way it ever hears back.
            setTimeout(() => reject(new Error("A browser cannot install an update.")), 800);
          }
        };
        setTimeout(tick, 300);
      }),
    onUpdateProgress: (listener) => {
      updateProgressListeners.add(listener);
      return () => {
        updateProgressListeners.delete(listener);
      };
    },
    masterLimiter: () => wait({ ...limiter }),
    setMasterLimiter: (wanted) => {
      limiter = {
        enabled: wanted.enabled,
        ceilingDb: Number.isFinite(wanted.ceilingDb)
          ? Math.min(Math.max(wanted.ceilingDb, -12), 0)
          : -0.3,
        releaseMs: Number.isFinite(wanted.releaseMs)
          ? Math.min(Math.max(wanted.releaseMs, 10), 1000)
          : 100,
      };
      return wait({ ...limiter });
    },
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
    deckMetronome: () => wait(undefined),
    deckKeyShift: (deck, semitones) => {
      (deck === "b" ? deckB : deckA).keyShift = Math.max(-12, Math.min(12, Math.round(semitones)));
      sendTick();
      return wait(undefined);
    },
    setMetronome: () => wait(undefined),
    setAudioConfig: () => wait(undefined),
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
    appVersion: () => wait("0.4.0"),
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
    // The fake volumes never come or go.
    onDevicesChanged: () => () => undefined,

    onMenu: (listener) => {
      const w = window as unknown as { __menu?: (id: string) => void };
      w.__menu = listener;
      return () => {
        delete w.__menu;
      };
    },

    // No network in a browser, so LINK cannot turn on. Saying why is better
    // than a switch that silently does nothing.
    linkStatus: () => wait(mockLinkStatus()),
    linkPeers: () => wait(linkMode === null || linkMode === "blocked" ? [] : mockPeers),
    onLinkPeers: () => () => undefined,
    startLinkExport: () =>
      wait(
        linkMode === null
          ? {
              ...linkOff(),
              problem: "LINK needs the desktop application; a browser has no access to the network.",
            }
          : mockLinkOn(),
      ),
    stopLinkExport: () => wait(linkOff()),
    loadTrackOnLink: () => wait(undefined),
    setLinkMaster: (on) => {
      mockMaster.on = on;
      return wait(mockLinkStatus());
    },
    nudgeLinkMaster: (deltaBpm) => {
      mockMaster.bpm = Math.min(300, Math.max(40, Math.round((mockMaster.bpm + deltaBpm) * 100) / 100));
      return wait(mockLinkStatus());
    },
    takeLinkMasterTempo: () => {
      // A mock master player runs at 128.00; take it.
      mockMaster.bpm = 128;
      return wait(mockLinkStatus());
    },
    onLinkStatus: () => () => undefined,

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
            beats: Math.round((row.durationSec * (row.bpmX100 || 12_800)) / 6000),
            peak: 0.9,
            durationSec: row.durationSec,
            elapsedMs: ANALYSIS_MS,
            analysisPath: `/PIONEER/USBANLZ/P${String(index % 1000).padStart(3, "0")}/${index.toString(16).toUpperCase().padStart(8, "0")}/ANLZ0000.DAT`,
          }),
          ANALYSIS_MS,
        ),
      );
    },

    // No picker in a browser, so nothing can be chosen to import or written.
    importFiles: () => wait(null),
    importXml: () => wait(null),
    exportPlaylistFile: () => wait(null),
    exportXml: () => wait(null),

    // Nothing in the mock has a file behind it, so nothing can be missing and
    // there is no picker to choose one with.
    missingTracks: () => wait({ total: 0, tracks: [] }),
    // The mock's titles are drawn from a short list, so the same title under
    // the same artist comes up as it does in a real library.
    findDuplicates: (limit) => {
      const groups = new Map<string, RowDto[]>();
      for (const row of all) {
        const key = `${row.title.toLowerCase()}\u0000${row.artist.toLowerCase()}`;
        groups.set(key, [...(groups.get(key) ?? []), row]);
      }
      const found = [...groups.values()].filter((rows) => rows.length > 1);
      return wait({
        groups: found.length,
        extra: found.reduce((n, rows) => n + rows.length - 1, 0),
        shown: found.slice(0, limit).map((rows) => ({
          title: rows[0]?.title ?? "",
          artist: rows[0]?.artist ?? "",
          tracks: rows.map((row) => ({ id: row.id, path: `/Music/${row.title}.mp3`, durationSec: row.durationSec, present: true })),
        })),
      });
    },
    relocateTrack: () => wait(null),
    // No files behind the rows, so nothing is missing and nothing moves.
    autoRelocate: () => wait({ relocated: 0, unresolved: 0 }),
    // No dialogs in a browser: the folder is a fixed one, so the search
    // folders list can be driven end to end.
    pickFolder: () => wait("/Users/mock/Music/Moved"),
    pickImage: () => wait("/Users/mock/Pictures/cover.jpg"),
    // No windows in a browser: the shell draws the Preferences over itself,
    // and its resets are its own to do.
    openPreferences: () => wait(false),
    // In a browser the Preferences overlay and the shell share a page, so a
    // request is handed straight to whoever is listening.
    onPreferencesReset: (listener) => {
      preferencesRequestListeners.add(listener);
      return () => {
        preferencesRequestListeners.delete(listener);
      };
    },
    requestPreferencesReset: (what) => {
      for (const listener of preferencesRequestListeners) listener(what);
      return wait(undefined);
    },
    closeWindow: () => wait(undefined),
    referenceStickSettings: () => {
      const reference = referenceDeviceSettings("", false);
      return wait({ categories: reference.categories, sorts: reference.sorts });
    },

    // The device tabs. Held per stick so a change survives switching tabs
    // and devices, the way a written file would.
    deviceSettings: (path) => wait(copySettings(settingsOf(path))),
    // The mock's sticks carry their settings from the start, so there is
    // nothing to give; what the stick holds comes back.
    writeDeviceDefaults: (path) => wait(copySettings(settingsOf(path))),
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
    reloadLibrary: () => bump(),
    // The mock's analysis rewrites no files, so nothing redraws.
    onAnalysisChanged: () => () => undefined,

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
    // The fake disk above. Copies, as with the tree: the map is the mock's.
    explorerRoots: () => wait(EXPLORER_ROOTS.map((root) => ({ ...root }))),
    explorerChildren: (path) => {
      const names = [...(EXPLORER_CHILDREN.get(path) ?? [])];
      // The stick's PIONEER folder stands in for one the cap cut: the real
      // backend keeps the first two thousand of a 14,503-folder card.
      return wait({ names, total: path === "/Volumes/SD/PIONEER" ? 14_503 : names.length });
    },
    trackDetails: (trackId) => {
      if (!ready) return notReady();
      // Counted for the deck's test: its INFO tab must not fetch a record
      // for every load when the tab is not showing.
      if (typeof window !== "undefined") {
        const w = window as unknown as { __detailsFetches?: number };
        w.__detailsFetches = (w.__detailsFetches ?? 0) + 1;
      }
      const row = all.find((r) => r.id === trackId);
      if (!row) return Promise.reject(new Error("That track is no longer in the library."));
      // A copy: the panel must not be able to edit the backend's own record.
      return wait({ ...detailsOf(row) });
    },
    trackLookups: () => wait({ keys: [...KEYS], genres: GENRES.filter((g) => g !== "") }),
  };
}

/** A bare `?name` flag in the URL. */
function readFlagFromUrl(name: string): boolean {
  if (typeof location === "undefined") return false;
  return new URLSearchParams(location.search).has(name);
}

/**
 * `?link=detected|on|blocked` puts the mock on a Pro DJ LINK network, which a
 * browser has no way to be on. `detected` hears two players and a mixer with
 * LINK off, `on` serves them, `blocked` reports the ports held.
 */
function readLinkFromUrl(): "detected" | "on" | "blocked" | null {
  if (typeof location === "undefined") return null;
  const raw = new URLSearchParams(location.search).get("link");
  return raw === "detected" || raw === "on" || raw === "blocked" ? raw : null;
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
function readLatencyFromUrl(): number | null {
  if (typeof location === "undefined") return null;
  const raw = new URLSearchParams(location.search).get("latency");
  const n = raw ? Number.parseInt(raw, 10) : NaN;
  return Number.isFinite(n) && n >= 0 && n <= 2000 ? n : null;
}
