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

export interface Backend {
  librarySummary(): Promise<LibrarySummary>;
  playlistTree(): Promise<TreeNode[]>;
  openView(spec: ViewSpec): Promise<ViewHandle>;
  fetchRows(viewId: number, offset: number, len: number): Promise<RowDto[]>;
  /** Ids between two row indices inclusive; used for shift-click across unfetched rows. */
  viewIdsInRange(viewId: number, from: number, to: number): Promise<string[]>;
}
