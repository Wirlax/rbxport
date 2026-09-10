/**
 * IPC contract.
 *
 * These types will be generated from the Rust DTOs by specta once `rbl-app`
 * lands; until then they are hand-written and the mock backend is checked
 * against the Rust view tests. Field names are camelCase to match
 * `#[serde(rename_all = "camelCase")]` on the Rust side.
 */

/** One row of the track table. Kept flat and small: ~200 bytes of JSON. */
export interface RowDto {
  id: string;
  trackNo: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  label: string;
  comment: string;
  /** BPM x100, as rekordbox stores it. */
  bpmX100: number;
  key: string;
  durationSec: number;
  rating: number;
  /** 0 = not analysed, 1 = analysed. */
  analysed: number;
  dateAdded: string;
  releaseDate: string;
  /** Hot cue letters present on the track, e.g. "ABCD". */
  cues: string;
  artworkHue: number;
  /** Whether the backend can serve artwork for this track. */
  hasArtwork: boolean;
}

export type TrackSource =
  | { kind: "collection" }
  | { kind: "playlist"; id: string }
  | { kind: "history"; id: string };

export type SortColumn =
  | "trackNo" | "title" | "artist" | "album" | "genre" | "label"
  | "bpm" | "key" | "duration" | "rating" | "dateAdded" | "releaseDate";

export interface ViewSpec {
  source: TrackSource;
  sort: SortColumn;
  descending: boolean;
  /** Free-text search, matched the way the Rust index folds it. */
  query: string;
  /**
   * The track filter bar's picks, applied by Rust after the search. Absent
   * means no filter; the sub-browser never sends one.
   */
  filter?: TrackFilter;
}

export interface ViewHandle {
  viewId: number;
  /** Row count for this view. */
  len: number;
  /** Bumped whenever the underlying library changes; stale pages are dropped. */
  gen: number;
}

export interface TreeNode {
  id: string;
  name: string;
  kind: "collection" | "histories" | "folder" | "playlist" | "history" | "allTracks" | "device";
  depth: number;
  /** Undefined for leaves. */
  expanded?: boolean;
  childCount?: number;
}

export interface LibrarySummary {
  trackCount: number;
  playlistCount: number;
  /** True when rekordbox is running, i.e. writes are refused. */
  readOnly: boolean;
  /** rekordbox's own DB schema version, e.g. 6000. */
  dbVersion: number | null;
}

/** What the app is costing, for the title bar's readout. */
export interface Diagnostics {
  /** Percent of one core, as the OS accounts it. Over 100 on several cores. */
  cpu: number;
  memoryMb: number;
  /** `null` where the platform will not say. */
  threads: number | null;
  openFiles: number | null;
  /**
   * Always `null` on macOS: per-process GPU is behind `powermetrics`, which
   * needs root. Reported rather than dropped, so the readout can say it is
   * unavailable instead of implying the app uses none.
   */
  gpu: number | null;
}

/** Every command returns this shape on failure. */
export interface AppErrorDto {
  kind: "readOnly" | "notFound" | "malformed" | "cancelled" | "internal";
  message: string;
  detail?: string;
}

/** Which waveform to fetch for a track. */
/**
 * Which waveform tag to read.
 *
 * `bands` and `bandsDetail` are the three-band ones rekordbox 7 draws — every
 * track checked in the reference library has them. The others are what a
 * library analysed before those existed would carry.
 */
export type WaveformKind = "bands" | "bandsDetail" | "preview" | "detail" | "colour";

export interface Backend {
  librarySummary(): Promise<LibrarySummary>;
  playlistTree(): Promise<TreeNode[]>;
  openView(spec: ViewSpec): Promise<ViewHandle>;
  fetchRows(viewId: number, offset: number, len: number): Promise<RowDto[]>;
  /** Ids between two row indices inclusive; used for shift-click across unfetched rows. */
  viewIdsInRange(viewId: number, from: number, to: number): Promise<string[]>;
  /**
   * Raw waveform bytes for a track, or an empty array when it has no analysis.
   * Sent as bytes rather than JSON: a colour waveform is several kilobytes of
   * numbers and JSON would multiply that and cost a parse on the UI thread.
   */
  trackWaveform(
    trackId: string,
    kind: WaveformKind,
    /** Window into the tag, in entries. The detail tag is far past the cap. */
    window?: { from: number; len: number },
  ): Promise<Uint8Array>;

  /**
   * Editing. Each returns the library's new generation, which invalidates every
   * cached page: the backend re-reads the library after a write.
   *
   * All of these are refused while Rekordbox is running — it holds the
   * database — and the refusal arrives as an `AppError` of kind `readOnly`.
   */
  edits: Edits;

  /**
   * Called when the library changes underneath us — after an edit, or when
   * Rekordbox itself writes. The argument is the new generation, which
   * invalidates every cached page.
   *
   * Returns an unsubscribe function.
   */
  onLibraryChanged(listener: (generation: number) => void): () => void;

  /**
   * Fires when the backend has finished loading the library.
   *
   * The load runs on its own thread and takes as long as the collection is
   * big, so the first thing the interface asks for can easily arrive before
   * there is anything to answer with. Without this the window sat on
   * "Loading…" forever, because the failed first attempt was never retried.
   *
   * Returns an unsubscribe function.
   */
  onLibraryReady(listener: () => void): () => void;

  /** Fires when the library could not be loaded at all, with the reason. */
  onLibraryError(listener: (message: string) => void): () => void;

  /**
   * Fires after a cue edit with the id of the track whose cues changed.
   * A deck showing that track refetches its cues; nothing else has to move.
   */
  onCuesChanged(listener: (trackId: string) => void): () => void;

  /**
   * Tracks whose file has gone. The count is exact; the list is a first page,
   * because a library can lose thousands when a drive is unplugged and a list
   * that long is neither useful nor small enough for the IPC cap.
   */
  /**
   * Analyses one track: tempo, beat grid, key and waveforms.
   *
   * Slow — a decode and a DSP pass — so callers run these one at a time.
   */
  analyseTrack(trackId: string): Promise<AnalysisResult>;

  /**
   * A track's whole beat grid, as raw bytes: five per beat, a little-endian
   * `u32` of milliseconds and the beat's number in its bar.
   *
   * Whole and once per track rather than a window at a time. Windowing it
   * still re-read and re-parsed the entire analysis file on every fetch, which
   * is the expensive part; `parseBeatGrid` turns these bytes into typed arrays
   * and `beatsIn` slices the window being drawn.
   */
  trackBeats(trackId: string): Promise<Uint8Array>;

  /** A track's cue points, ordered by position. */
  trackCues(trackId: string): Promise<Cue[]>;

  /**
   * Asks for files and adds them to the library.
   *
   * Resolves to what happened, or `null` if the picker was cancelled. Reports
   * per file rather than failing the batch: a folder with two unreadable
   * tracks should import the rest.
   */
  importFiles(): Promise<ImportReport | null>;

  /**
   * Writes a playlist to `destination`, asking for one when none is given.
   *
   * Resolves to what was written, or `null` if the picker was cancelled. A
   * destination that already holds one of our exports is synced rather than
   * rewritten.
   */
  exportPlaylist(playlistId: string, destination?: string): Promise<ExportReport | null>;

  /** A track's phrase structure, empty when it has no `PSSI` tag. */
  trackPhrases(trackId: string): Promise<Phrase[]>;

  /**
   * Per-column vocal presence, empty when the track has no `PVDI` tag.
   *
   * Raw bytes rather than JSON: this is one value per overview column.
   */
  trackVocals(trackId: string): Promise<Uint8Array>;

  /** The volumes an export could be written to, and what is on each. */
  listDevices(): Promise<Device[]>;

  /**
   * Native menu clicks, as the item's id.
   *
   * One subscription rather than one per item, so adding a menu item does not
   * mean adding another listener. Returns its own unsubscribe.
   */
  onMenu(listener: (id: string) => void): () => void;

  /**
   * Listens for devices on the link network. Listen-only — nothing is
   * transmitted, because announcing as a source needs the database server's
   * menus, which are not built.
   */
  startLinkListening(): Promise<LinkStatus>;
  stopLinkListening(): Promise<void>;
  /** Called as the set of devices on the network changes. */
  onLinkPeers(listener: (peers: LinkPeer[]) => void): () => void;

  /**
   * The decks.
   *
   * Playback is in Rust — see `crates/rbl-deck`. The audio device is not
   * opened until one of these is called, so a window nobody has played
   * anything in holds no device at all.
   *
   * Position does not come back from any of these: it arrives on `onDeckTick`
   * ten times a second and the interface extrapolates between ticks.
   */
  deckLoad(deck: DeckId, trackId: string): Promise<void>;
  deckUnload(deck: DeckId): Promise<void>;
  deckPlay(deck: DeckId): Promise<void>;
  deckPause(deck: DeckId): Promise<void>;
  deckSeek(deck: DeckId, positionMs: number): Promise<void>;
  /**
   * Dragging the waveform like a record.
   *
   * Between `deckScrubBegin` and `deckScrubEnd` the deck reads a decoded
   * window at the drag's own rate — forwards, backwards, and silent when the
   * pointer stops — rather than seeking. A seek per pointer move gives the
   * right place at the wrong speed: a burst of normal-speed audio each time.
   */
  deckScrubBegin(deck: DeckId): Promise<void>;
  deckScrubTo(deck: DeckId, positionMs: number): Promise<void>;
  deckScrubEnd(deck: DeckId): Promise<void>;
  /** The master output level, 0 to 1. It arrives back on the next tick. */
  setMasterLevel(level: number): Promise<void>;
  /**
   * The outputs the audio could go to, and which is in use.
   *
   * Read on each call rather than cached: an interface is plugged in while the
   * app is open more often than not.
   */
  audioDevices(): Promise<AudioDevices>;
  /** Choose one, or `null` for the system's own. Takes effect on the next play. */
  setAudioDevice(device: string | null): Promise<void>;
  /**
   * How fast a deck plays, as a multiple of the file's own speed.
   *
   * A ratio rather than a BPM: what BPM that comes to depends on the track,
   * and the deck does not need to know the track's to play it faster.
   */
  deckTempo(deck: DeckId, tempo: number): Promise<void>;
  /** Master Tempo: whether the pitch is held while the speed changes. */
  deckMasterTempo(deck: DeckId, on: boolean): Promise<void>;
  /**
   * One deck's channel strip.
   *
   * Knob positions rather than decibels: what a position means is the mixer's
   * to decide and it changes with the EQ / ISOLATOR switch, so the interface
   * never has to know the curve. 0.5 is centre and 0.5 is unity.
   */
  setChannelBand(deck: DeckId, band: EqBand, position: number): Promise<void>;
  /** Kill a band: the knob at the bottom of whichever curve is in use. */
  setChannelKill(deck: DeckId, band: EqBand, killed: boolean): Promise<void>;
  /** The deck's gain, 0 to 2 — up to +6 dB, as a mixer's trim gives. */
  setChannelTrim(deck: DeckId, trim: number): Promise<void>;
  /** The crossfader: 0 is deck A alone, 1 is deck B alone, 0.5 is both. */
  setCrossfade(position: number): Promise<void>;
  /** EQ or ISOLATOR — the bottom of each band's travel, and nothing else. */
  setEqCurve(isolator: boolean): Promise<void>;
  /** Both decks now, to anchor the interface when it starts. */
  deckState(): Promise<Tick>;
  /** Both decks, ten times a second, and only while something is playing. */
  onDeckTick(listener: (tick: Tick) => void): () => void;
  /**
   * The master's meters, thirty times a second.
   *
   * Its own event because it is wanted three times as often as the decks and
   * is a twentieth of the size. The peaks are cleared as they are read, so
   * each one is the loudest sample since the last.
   */
  onMeters(listener: (meters: Meters) => void): () => void;
  /** A deck has finished loading a track, or could not. */
  onDeckEvent(listener: (event: DeckEvent) => void): () => void;

  /** A reading of this process, sampled on demand. */
  appDiagnostics(): Promise<Diagnostics>;

  /** Shows a track's file in the Finder. */
  revealTrack(trackId: string): Promise<void>;

  missingTracks(limit: number): Promise<MissingTracks>;

  /**
   * Asks the user for a file and points a track at it.
   *
   * Resolves to the chosen path, or `null` if they cancelled. Outside Tauri
   * there is no picker, so it resolves to `null` immediately.
   */
  relocateTrack(trackId: string): Promise<string | null>;

  /**
   * The values the track filter bar can offer for a list: which whole BPMs
   * and which keys it holds, counted over the source and query alone so a
   * picked value never hides the others. Rust tallies them in one pass; the
   * frontend never scans rows for this.
   */
  filterValues(spec: ViewSpec): Promise<FilterValues>;

  /**
   * What a selected device's six tabs show: the display settings a player
   * reads from `DEVSETTING.DAT`, and the name, browse categories, sort
   * options, sub-column and colour comments in `exportLibrary.db`. A stick
   * that holds none of it still answers, with the defaults and the presence
   * flags clear.
   */
  deviceSettings(path: string): Promise<DeviceSettings>;
  /** Writes them back and resolves to what the stick now holds. */
  saveDeviceSettings(path: string, settings: DeviceSettings): Promise<DeviceSettings>;
}

/** Which deck. Two, named rather than indexed, as the mixer is. */
export type DeckId = "a" | "b";

/** One deck in a tick. */
/** One output the audio could go to. */
export interface AudioDevice {
  /** What to store and open by: stable across runs and reboots. */
  id: string;
  /** What to show. */
  name: string;
}

export interface AudioDevices {
  devices: AudioDevice[];
  /** The system's own choice, so the list can say which that is. */
  default: string | null;
  /** What this app has been told to use, or `null` for the system's. */
  chosen: string | null;
}

/** The three bands of a channel strip, high to low as the strip is drawn. */
export type EqBand = "high" | "mid" | "low";

export interface DeckTick {
  /** The engine's frame counter, in device-rate frames. */
  frames: number;
  /** The track's length in the same frames, or 0 when it is not known. */
  totalFrames: number;
  /** Bumped on every load and seek; a new one means snap, not slide. */
  generation: number;
  playing: boolean;
  loaded: boolean;
  /** A multiple of the file's own speed: 1 is the track as recorded. */
  tempo: number;
  /** Whether the pitch is held while that speed changes. */
  masterTempo: boolean;
}

/** Both decks at one instant. About 200 bytes, well inside the event cap. */
export interface Tick {
  a: DeckTick;
  b: DeckTick;
  /** The device's rate, which is what every frame count here is in. */
  sampleRate: number;
  /** The loudest sample the device was given last callback, per channel. */
  peakLeft: number;
  peakRight: number;
  /** The master level, 0 to 1. */
  master: number;
}

/** The master's meters and level, on their own faster beat. */
export interface Meters {
  peakLeft: number;
  peakRight: number;
  master: number;
}

/** A deck finishing a load, or failing one. */
export interface DeckEvent {
  deck: DeckId;
  totalFrames: number;
  sampleRate: number;
  /** Set when the load failed, and says why. */
  message: string | null;
}

/**
 * One cue point.
 *
 * No colour: what colour rekordbox draws a cue is decided by
 * `djmdCue.ColorTableIndex`, which is not understood, so none is reported
 * rather than a guessed one.
 */
export interface Cue {
  /**
   * `djmdCue.ID`, which `moveCue` and `deleteCue` take. Empty for a cue the
   * backend cannot edit — one whose id is not a number under 2^32, of which
   * the reference library has none — and the interface offers no ✕ for it.
   */
  id: string;
  positionMs: number;
  /** Where a loop ends, or 0 for a plain cue. */
  outMs: number;
  /** `A` to `P` for a hot cue, empty for a memory cue. */
  letter: string;
  memory: boolean;
}

/** Which slot a new cue goes in: a memory cue, or a hot cue by its letter. */
export type CueKind = "memory" | { hot: string };

/** A volume an export could be written to. */
export interface Device {
  name: string;
  /** Where it is mounted; this is what an export is written to. */
  path: string;
  totalBytes: number;
  freeBytes: number;
  /** Whether the OS calls it removable. External SSDs often say no. */
  removable: boolean;
  /** What is already on it, null when it holds no export. */
  export: DeviceExport | null;
}

/** What a device already holds. */
export interface DeviceExport {
  tracks: number;
  playlists: number;
  /** True when we wrote it, which is what makes the next export a sync. */
  ours: boolean;
  /** When our own export last ran; empty when this is not one of ours. */
  written: string;
}

/** What an export wrote. */
export interface ExportReport {
  tracks: number;
  playlists: number;
  bytesCopied: number;
  analysisFiles: number;
  /** Tracks already on the stick, unchanged, that did not need copying again. */
  reused: number;
  /** Tracks taken off the stick because the playlist no longer holds them. */
  removed: number;
  /** Tracks left out because their audio was missing or unreadable. */
  skipped: string[];
  /** Whether the export read back correctly with the independent parser. */
  verified: boolean;
}

/**
 * One phrase of a track's structure, from the `PSSI` analysis tag.
 *
 * `label` is what rekordbox prints in the phrase bar — "INTRO 2", "CHORUS 1",
 * "UP 1" — and `kind` is the raw numeric value it came from, kept so a label
 * we do not recognise can still be traced back.
 */
export interface Phrase {
  /** Beat the phrase starts on, 1-based. */
  beat: number;
  /**
   * Milliseconds from the start, resolved against the beat grid.
   *
   * Absent when the grid does not reach the phrase, which happens on a track
   * whose analysis is older than its length. The strip falls back to the beat.
   */
  timeMs: number | null;
  kind: number;
  label: string;
}

/** One beat of the grid. */
export interface Beat {
  timeMs: number;
  /** The first beat of a bar, drawn heavier than the rest. */
  downbeat: boolean;
}

/** A device heard on the Pro DJ Link network. */
export interface LinkPeer {
  name: string;
  deviceNumber: number;
  kind: string;
  address: string;
  lastSeenMs: number;
}

export interface LinkStatus {
  listening: boolean;
  /** Why not, when it is not. */
  problem: string | null;
  peers: LinkPeer[];
}

/** What analysing one track found. */
export interface AnalysisResult {
  trackId: string;
  bpmX100: number;
  key: string;
  /** How long the analysis took, for the progress readout. */
  elapsedMs: number;
}

/** What an import batch did. */
export interface ImportReport {
  imported: number;
  /** One line per file that was not imported, saying why. */
  skipped: string[];
}

/** A track whose audio file is no longer where the library says. */
export interface MissingTrack {
  id: string;
  title: string;
  artist: string;
  /** Where the library still expects it. */
  path: string;
}

export interface MissingTracks {
  /** Every missing track, not just the ones listed. */
  total: number;
  tracks: MissingTrack[];
}

export interface Edits {
  createPlaylist(name: string, parent: string): Promise<number>;
  createFolder(name: string, parent: string): Promise<number>;
  renamePlaylist(id: string, name: string): Promise<number>;
  movePlaylist(id: string, parent: string): Promise<number>;
  deletePlaylist(id: string): Promise<number>;
  addTracksToPlaylist(playlist: string, tracks: string[]): Promise<number>;
  removeTracksFromPlaylist(playlist: string, tracks: string[]): Promise<number>;
  reorderPlaylist(playlist: string, tracks: string[]): Promise<number>;
  setTrackRating(track: string, stars: number): Promise<number>;
  setTrackComment(track: string, comment: string): Promise<number>;
  setTrackColor(track: string, color: string | null): Promise<number>;

  /**
   * Cues. Unlike the edits above these do not return a generation: a cue
   * edit changes one track's cues and nothing else, so the backend re-reads
   * only that track and says so through `onCuesChanged` rather than
   * reloading the library and dropping every cached page.
   */
  /** Adds a cue and resolves to its id. */
  addCue(track: string, kind: CueKind, positionMs: number): Promise<string>;
  /**
   * Adds a loop: a cue with an out point. `beats` is the loop's length in
   * beats when known; left out, In and Out are all that is recorded, which
   * is what most of the library's loops do.
   */
  addLoop(track: string, kind: CueKind, inMs: number, outMs: number, beats?: number): Promise<string>;
  moveCue(cue: string, positionMs: number): Promise<void>;
  deleteCue(cue: string): Promise<void>;
}

/** `ParentID` of a playlist or folder at the top of the tree. */
export const TREE_ROOT = "root";

/**
 * The BPM column of the track filter bar.
 *
 * `values` are whole BPMs picked from the list, empty for `All`. With values
 * picked, `tolerancePct` widens each into a band; with none, it is a band
 * around the master player's BPM — and with no master player it is inert.
 */
export interface BpmFilter {
  values: number[];
  /** 0 to 6, the `MASTER PLAYER ± n%` list. */
  tolerancePct: number;
  /** The master player's BPM x100, or `null` when no deck is loaded. */
  masterBpmX100: number | null;
}

/**
 * The track filter bar's picks: one entry per ticked column, combined with
 * AND. Keys and colours travel by name, which is what the bar shows.
 */
export interface TrackFilter {
  bpm?: BpmFilter;
  keys?: string[];
  ratings?: number[];
  colors?: string[];
}

/** A value the bar can offer, with how many tracks of the list carry it. */
export interface Counted<T> {
  value: T;
  count: number;
}

/** A My Tag category and the tags under it, by name. Drawn, not filtered on. */
export interface TagCategory {
  name: string;
  tags: string[];
}

export interface FilterValues {
  /** Whole BPMs present, ascending. */
  bpms: Counted<number>[];
  /** Keys present, in Camelot order. */
  keys: Counted<string>[];
  tags: TagCategory[];
}

/** One browse category or sort option on a stick: a row of `category` or `sort`. */
export interface MenuSlot {
  /** The row's own key, stable across exports. */
  id: number;
  /** Which menu item it is; `MENU_ITEM.*` in `src/lib/deviceSettings.ts`. */
  menuItem: number;
  /** As the player shows it: `ARTIST`, `DATE ADDED`. */
  name: string;
  /** Position among the visible rows, 1-based; 0 when hidden. */
  seq: number;
  visible: boolean;
}

export interface ColorName {
  /** 1 to 8, in rekordbox's order Pink to Purple. */
  id: number;
  name: string;
}

export type WaveformColor = "blue" | "rgb" | "3band";
export type WaveformPosition = "center" | "left";
export type OverviewWaveform = "half" | "full";
export type KeyDisplay = "classic" | "alphanumeric";

/** Everything the device tabs read and write. A few kilobytes. */
export interface DeviceSettings {
  /** `export.pdb` is on the stick — "Device Library" in rekordbox's words. */
  hasDeviceLibrary: boolean;
  /** `exportLibrary.db` is on the stick — "OneLibrary". */
  hasOneLibrary: boolean;
  /** `DEVSETTING.DAT` was read; when false the four below are defaults. */
  hasDevSetting: boolean;
  waveformColor: WaveformColor;
  waveformPosition: WaveformPosition;
  overviewWaveform: OverviewWaveform;
  keyDisplay: KeyDisplay;
  /** The library rows were read; when false they are the reference rows and are not written. */
  hasLibrarySettings: boolean;
  deviceName: string;
  /** `property.backGroundColorType`, carried but not understood. */
  backgroundColorType: number;
  categories: MenuSlot[];
  sorts: MenuSlot[];
  /** `menuItem` of the sort option shown beside the track name, or null for Not Specified. */
  subColumn: number | null;
  colors: ColorName[];
}
