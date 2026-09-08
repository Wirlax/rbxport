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
  kind: "collection" | "folder" | "playlist" | "history" | "allTracks" | "device";
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
   * A track's beats within a window, from its analysis file.
   *
   * Windowed because a long mix has tens of thousands and the whole grid
   * would blow the IPC cap.
   */
  trackBeats(trackId: string, fromMs: number, toMs: number): Promise<Beat[]>;

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

  missingTracks(limit: number): Promise<MissingTracks>;

  /**
   * Asks the user for a file and points a track at it.
   *
   * Resolves to the chosen path, or `null` if they cancelled. Outside Tauri
   * there is no picker, so it resolves to `null` immediately.
   */
  relocateTrack(trackId: string): Promise<string | null>;
}

/**
 * One cue point.
 *
 * No colour: what colour rekordbox draws a cue is decided by
 * `djmdCue.ColorTableIndex`, which is not understood, so none is reported
 * rather than a guessed one.
 */
export interface Cue {
  positionMs: number;
  /** `A` to `P` for a hot cue, empty for a memory cue. */
  letter: string;
  memory: boolean;
}

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
}

/** `ParentID` of a playlist or folder at the top of the tree. */
export const TREE_ROOT = "root";
