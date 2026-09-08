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
  kind: "collection" | "folder" | "playlist" | "history" | "allTracks";
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
export type WaveformKind = "preview" | "detail" | "colour";

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
  trackWaveform(trackId: string, kind: WaveformKind): Promise<Uint8Array>;

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
   * Tracks whose file has gone. The count is exact; the list is a first page,
   * because a library can lose thousands when a drive is unplugged and a list
   * that long is neither useful nor small enough for the IPC cap.
   */
  missingTracks(limit: number): Promise<MissingTracks>;
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
