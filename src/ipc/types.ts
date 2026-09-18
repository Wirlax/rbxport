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
  /**
   * The track's hot cues, in slot order A to P, for the badges on the row's
   * preview waveform. Tuples rather than objects so a page of 64 rows with
   * every slot set stays inside the 64 KB response cap.
   */
  hotCues: RowCue[];
  artworkHue: number;
  /** Whether the backend can serve artwork for this track. */
  hasArtwork: boolean;
  /**
   * The file's own name, for the Explorer's File Name column.
   *
   * Optional only so a row built before the column existed still type-checks;
   * the backend always sends it.
   */
  fileName?: string;
}

export type TrackSource =
  | { kind: "collection" }
  | { kind: "playlist"; id: string }
  | { kind: "history"; id: string }
  /** A folder on disk, for the Explorer. An empty path is the heading, which lists nothing. */
  | { kind: "folder"; path: string };

export type SortColumn =
  | "trackNo" | "title" | "artist" | "album" | "genre" | "label"
  | "bpm" | "key" | "duration" | "rating" | "dateAdded" | "releaseDate";

/**
 * What the backend sorts by. The columns, plus the key round the Camelot
 * wheel: the Key column sorts by what it shows, and with the alphanumeric
 * display that is `1A` to `12B`, not the classic names' alphabet.
 */
export type SortKey = SortColumn | "keyCamelot";

export interface ViewSpec {
  source: TrackSource;
  sort: SortKey;
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
  kind:
    | "collection" | "histories" | "folder" | "playlist" | "history" | "allTracks" | "device"
    /** An intelligent playlist: a rule, whose tracks are whatever it admits when opened. */
    | "smartPlaylist"
    /** The Explorer heading, and a folder on disk under it. */
    | "explorer" | "directory"
    /** A line of information in the tree, not a place: nothing opens when it is clicked. */
    | "note";
  depth: number;
  /** Undefined for leaves. */
  expanded?: boolean;
  childCount?: number;
  /**
   * Its children are read when it is opened, not before, so it is a branch
   * whether or not anything sits under it yet. The Explorer's folders.
   */
  lazy?: true;
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
 * Which waveform tag to read, one pair per palette of View › Color:
 *
 * - `bands` / `bandsDetail`: the three-band `PWV6` / `PWV7`, three bytes a
 *   column, which 3Band draws — every track checked in the reference
 *   library has them.
 * - `mono` / `monoDetail`: `PWAV` / `PWV3`, one byte a column (five bits of
 *   height, three of whiteness), which BLUE draws.
 * - `colour` / `colourDetail`: `PWV4` / `PWV5`, six and two bytes a column,
 *   which RGB draws.
 */
export type WaveformKind = "bands" | "bandsDetail" | "mono" | "monoDetail" | "colour" | "colourDetail";

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
   * Re-reads the library on request. What the analysis queue asks for once
   * it has drained: the rows drawn from each answer are then replaced by
   * the library's own, in one reload rather than one per track.
   */
  reloadLibrary(): Promise<number>;
  /**
   * A track's analysis files were rewritten, so a deck showing it redraws
   * its waveform. Returns its own unsubscribe.
   */
  onAnalysisChanged(listener: (trackId: string) => void): () => void;

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
   * Export a playlist to a file, where the platform's save dialog says:
   * `m3u8`, or rekordbox's tab-separated `txt`. Resolves to how many tracks,
   * or null when cancelled.
   */
  exportPlaylistFile(playlistId: string, name: string, format: "m3u8" | "txt"): Promise<number | null>;
  /**
   * Imports a rekordbox XML collection chosen in the platform's file
   * dialog: its files into the library, its playlists, and the cues of each
   * track that landed. Null when the dialog is cancelled.
   */
  importXml(): Promise<XmlImportReport | null>;
  /**
   * Writes the collection as rekordbox's XML where the platform's save
   * dialog says; resolves to how many tracks, or null when cancelled.
   */
  exportXml(): Promise<number | null>;

  /**
   * Writes a playlist to `destination`, asking for one when none is given.
   *
   * Resolves to what was written, or `null` if the picker was cancelled. A
   * destination that already holds one of our exports is synced rather than
   * rewritten.
   */
  exportPlaylist(
    playlistId: string,
    destination?: string,
    /** What a stick with no settings of its own is given; see `StickDefaults`. */
    defaults?: StickDefaults,
  ): Promise<ExportReport | null>;

  /**
   * rekordbox's reference browse categories and sort options: what a
   * fresh export writes when the Preferences window has not changed them.
   */
  referenceStickSettings(): Promise<ReferenceStickSettings>;

  /** A track's phrase structure, empty when it has no `PSSI` tag. */
  trackPhrases(trackId: string): Promise<Phrase[]>;
  /**
   * PHRASE EDIT: `cut` splits the phrase under `beat` (1-based, on the
   * track's grid), `clear` takes it out. Resolves to whether anything
   * changed; `onAnalysisChanged` says so as well.
   */
  editPhrase(trackId: string, beat: number, action: "cut" | "clear"): Promise<boolean>;

  /**
   * Per-column vocal presence, empty when the track has no `PVDI` tag.
   *
   * Raw bytes rather than JSON: this is one value per overview column.
   */
  trackVocals(trackId: string): Promise<Uint8Array>;

  /** The backups this app has taken, newest first. */
  listBackups(): Promise<Backup[]>;
  /** Copies the library aside now; resolves to where the copy went. */
  backUpLibrary(): Promise<string>;
  /** Puts a backup back as the library and re-reads it. Refused while rekordbox runs. */
  restoreBackup(path: string): Promise<number>;
  /** Called after each track of an export, while one runs. Returns its own unsubscribe. */
  onExportProgress(listener: (progress: ExportProgress) => void): () => void;
  /** A yes-or-no question in the platform's own dialog; false when dismissed. */
  confirm(message: string): Promise<boolean>;
  /** The volumes an export could be written to, and what is on each. */
  listDevices(): Promise<Device[]>;
  /**
   * Called when a volume is mounted or unmounted, so the panel can refresh
   * without waiting for focus or a click. Returns its own unsubscribe.
   */
  onDevicesChanged(listener: () => void): () => void;

  /**
   * Native menu clicks, as the item's id.
   *
   * One subscription rather than one per item, so adding a menu item does not
   * mean adding another listener. Returns its own unsubscribe.
   */
  onMenu(listener: (id: string) => void): () => void;

  /** LINK as it stands: on or off, on what, and who is listening. */
  linkStatus(): Promise<LinkStatus>;
  /** Players and mixers heard on the network, whether or not LINK is on. */
  linkPeers(): Promise<LinkPeerSeen[]>;
  /** Called as the set of devices heard on the network changes (from start). */
  onLinkPeers(listener: (peers: LinkPeerSeen[]) => void): () => void;
  /**
   * Turns LINK on: the app announces itself as `rekordbox` on the named
   * interface (the first one when none is given) and serves the library to
   * every player that asks. Refused, with the reason, while rekordbox runs.
   */
  startLinkExport(iface?: string): Promise<LinkStatus>;
  stopLinkExport(): Promise<LinkStatus>;
  /** Tells a CDJ on the link to load a specific track from our library. */
  loadTrackOnLink(playerNumber: number, trackId: string): Promise<void>;
  /** Becomes the network's tempo master, or resigns; returns fresh status. */
  setLinkMaster(on: boolean): Promise<LinkStatus>;
  /** Nudges the master tempo by `deltaBpm` (rekordbox's −/+ is ±1). */
  nudgeLinkMaster(deltaBpm: number): Promise<LinkStatus>;
  /** Takes the current master player's tempo as the master tempo (⟳). */
  takeLinkMasterTempo(): Promise<LinkStatus>;
  /** Called as LINK turns on or off and as the players change. */
  onLinkStatus(listener: (status: LinkStatus) => void): () => void;

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
  /**
   * Starts a deck after `delayMs` of silence, counted by the audio callback:
   * quantized play on a synced deck, held for the master's next beat.
   */
  deckPlayAfter(deck: DeckId, delayMs: number): Promise<void>;
  deckPause(deck: DeckId): Promise<void>;
  deckSeek(deck: DeckId, positionMs: number): Promise<void>;
  /**
   * Sets a loop between two points and turns it on. A head already past
   * the out point goes back to the in point. The deck rounds at the out
   * point itself, on the frame, with nothing faded at the seam.
   */
  deckSetLoop(deck: DeckId, inMs: number, outMs: number): Promise<void>;
  /** RELOOP (on): back in from the in point. EXIT (off): out, the range kept. */
  deckLoopActive(deck: DeckId, on: boolean): Promise<void>;
  deckClearLoop(deck: DeckId): Promise<void>;
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
   * Asks the download server for the newest version.
   *
   * `version` is null when this build is the newest; otherwise `changes`
   * holds the changelog between the two, newest first, and the update is
   * held for `installUpdate`.
   */
  checkForUpdate(): Promise<UpdateCheck>;
  /**
   * Downloads and installs what the last check found, then restarts. It
   * resolves only if the install failed — a success restarts the app.
   */
  installUpdate(): Promise<void>;
  /** The download's progress, about ten times a second while it runs. */
  onUpdateProgress(listener: (progress: UpdateProgress) => void): () => void;
  /** The master limiter as it stands. */
  masterLimiter(): Promise<Limiter>;
  /** Sets the master limiter; what comes back is what the engine could set. */
  setMasterLimiter(limiter: Limiter): Promise<Limiter>;
  /**
   * How fast a deck plays, as a multiple of the file's own speed.
   *
   * A ratio rather than a BPM: what BPM that comes to depends on the track,
   * and the deck does not need to know the track's to play it faster.
   */
  deckTempo(deck: DeckId, tempo: number): Promise<void>;
  /** Master Tempo: whether the pitch is held while the speed changes. */
  deckMasterTempo(deck: DeckId, on: boolean): Promise<void>;
  /** A click on every beat of the deck's grid while it plays. */
  deckMetronome(deck: DeckId, on: boolean): Promise<void>;
  /** The key, in semitones from the track's own; −12 to 12. */
  deckKeyShift(deck: DeckId, semitones: number): Promise<void>;
  /** Preferences › Audio › Metronome: which click (1 to 3) and how loud. */
  setMetronome(sound: 1 | 2 | 3, volume: "small" | "middle" | "large"): Promise<void>;
  /**
   * Preferences › Audio › Sample Rate and Buffer size, asked of the device
   * the next time a deck plays. Either may be left to the device.
   */
  setAudioConfig(sampleRate: number | null, bufferFrames: number | null): Promise<void>;
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
  /** The version this build carries, for About; no network is asked. */
  appVersion(): Promise<string>;

  /** Shows a track's file in the Finder. */
  revealTrack(trackId: string): Promise<void>;

  missingTracks(limit: number): Promise<MissingTracks>;
  /** Tracks that share a title and an artist, the first `limit` groups listed. */
  findDuplicates(limit: number): Promise<Duplicates>;

  /**
   * Asks the user for a file and points a track at it.
   *
   * Resolves to the chosen path, or `null` if they cancelled. Outside Tauri
   * there is no picker, so it resolves to `null` immediately.
   */
  relocateTrack(trackId: string): Promise<string | null>;

  /**
   * Points every missing track at a file of the same name found under one
   * of `folders`, searched in order. A track whose name is found nowhere
   * is left missing.
   */
  autoRelocate(folders: string[]): Promise<RelocateReport>;

  /** Opens a folder picker; null when it is cancelled. */
  pickFolder(title: string): Promise<string | null>;
  /** The platform's file dialog for one JPEG or PNG; null when cancelled. */
  pickImage(title: string): Promise<string | null>;

  /**
   * Opens the Preferences window on `pane`, or turns the open one to it.
   * False where there are no windows — a browser — and the shell draws the
   * Preferences over itself instead.
   */
  openPreferences(pane: string): Promise<boolean>;

  /**
   * The Preferences window asking the main one for something only it holds:
   * the browser's columns or the panes' widths put back, or a check for
   * updates, whose window belongs to the main one. Returns its own
   * unsubscribe.
   */
  onPreferencesReset(listener: (what: PreferencesRequest) => void): () => void;
  requestPreferencesReset(what: PreferencesRequest): Promise<void>;

  /** Closes the window this runs in; nothing in a browser. */
  closeWindow(): Promise<void>;

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
  /**
   * Gives a stick that holds an export but no DEVSETTING.DAT the DJ System
   * defaults, as rekordbox does when its device panel opens; a stick that
   * has one keeps it. Resolves to what the stick now holds.
   */
  writeDeviceDefaults(path: string, defaults: StickDefaults): Promise<DeviceSettings>;
  /** Writes them back and resolves to what the stick now holds. */
  saveDeviceSettings(path: string, settings: DeviceSettings): Promise<DeviceSettings>;

  /**
   * The Explorer.
   *
   * Where it starts, and one folder's subfolders when that folder is opened.
   * Nothing is read ahead: a volume costs one directory read of its top
   * level until somebody opens something under it. A folder that cannot be
   * read answers with no children rather than an error.
   */
  explorerRoots(): Promise<ExplorerRoot[]>;
  explorerChildren(path: string): Promise<ExplorerChildren>;

  /**
   * One track's full record: what the information panel's Summary and Info
   * tabs show and the row DTO does not carry.
   *
   * A point read by id, not a widening of the index — the twenty-odd columns
   * are wanted for one track at a time. About 1 KB.
   */
  trackDetails(trackId: string): Promise<TrackDetails>;

  /** What the Info tab's Key and Genre dropdowns offer: what the library holds. */
  trackLookups(): Promise<TrackLookups>;
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

/** What the Preferences window can ask the main window to do. */
export type PreferencesRequest = "columns" | "layout" | "updates";

/** One release's section of the changelog. */
export interface UpdateChange {
  version: string;
  /** `2026-09-10`, when the heading carries one. */
  date: string | null;
  /** The section's markdown, its heading included. */
  body: string;
}

/** What a check for updates found. */
export interface UpdateCheck {
  currentVersion: string;
  /** The version on offer, or null when this build is the newest. */
  version: string | null;
  /** RFC 3339, when the feed says when it was published. */
  date: string | null;
  /** The changelog between the two versions, newest first. */
  changes: UpdateChange[];
}

export interface UpdateProgress {
  downloaded: number;
  /** null when the server did not say how big the file is. */
  total: number | null;
}

/**
 * The master limiter, which keeps two decks summed from clipping.
 *
 * Both directions: what is sent, and what the engine says it set — it clamps
 * the numbers to what it can do, and the interface shows that.
 */
export interface Limiter {
  enabled: boolean;
  /** dBFS, −12 to 0. */
  ceilingDb: number;
  /** Milliseconds, 10 to 1000. */
  releaseMs: number;
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
  /** Semitones from the track's own key: a CDJ's key shift. */
  keyShift: number;
  /** Output frames until a started deck sounds: a play held for the beat. */
  startInFrames: number;
  /** The loop's in and out points in the same frames; both 0 for none. */
  loopInFrames: number;
  loopOutFrames: number;
  /** Whether the deck is inside the loop: RELOOP on, EXIT off. */
  looping: boolean;
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
  /** How far the limiter turned the sum down since the last tick, in dB. */
  reduction: number;
  /**
   * Whether this build can shift a key without moving the tempo. Only the
   * Rubber Band backend holds a pitch to within a cent; without it the
   * semitone buttons are drawn greyed rather than offer a wrong key.
   */
  shiftsKey: boolean;
}

/** The master's meters and level, on their own faster beat. */
export interface Meters {
  peakLeft: number;
  peakRight: number;
  master: number;
  /** How far the limiter turned the sum down since the last tick, in dB. */
  reduction: number;
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
 * The colour is what rekordbox paints for the cue's `ColorTableIndex`, from
 * the nine indices measured off the captures; an index outside those arrives
 * as `null` and draws the default green rather than a guess. A memory cue has
 * no colour of its own and is always `null`.
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
  /** `#RRGGBB`, or `null` where unmeasured. */
  colour: string | null;
}

/** Which slot a new cue goes in: a memory cue, or a hot cue by its letter. */
export type CueKind = "memory" | { hot: string };

/** A hot cue on a track-list row: letter, position in ms, drawn colour. */
export type RowCue = readonly [letter: string, positionMs: number, colour: string | null];

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

/** A device heard on the network before LINK is on. */
export interface LinkPeerSeen {
  number: number;
  name: string;
  /** `player`, `mixer`, `rekordbox` or `device`. */
  kind: string;
  address: string;
}

/** A network interface LINK can run on. */
export interface LinkInterface {
  /** The OS name: `en0`, `Ethernet 2`. */
  name: string;
  address: string;
}

/** A player on the link, and what it has loaded from us. */
export interface LinkPlayer {
  number: number;
  name: string;
  /** `player`, `mixer`, `rekordbox` or `device`. */
  kind: string;
  address: string;
  loaded: { id: string; title: string; artist: string } | null;
  playing: boolean;
  master: boolean;
}

export interface LinkStatus {
  on: boolean;
  /** Why it could not be turned on, when it could not. */
  problem: string | null;
  /** What it runs on, while on. */
  interface: LinkInterface | null;
  players: LinkPlayer[];
  /** What it could run on, for the picker. */
  interfaces: LinkInterface[];
  /** We are the network's tempo master, driving the tempo the players sync to. */
  master: boolean;
  /** The master tempo we would drive, in BPM; shown whether or not we are master. */
  masterBpm: number;
}

/** One backup of the library. */
export interface Backup {
  path: string;
  /** The file's name, which carries when it was taken. */
  name: string;
  bytes: number;
}

/** Where an export has got to, after each track. */
export interface ExportProgress {
  done: number;
  total: number;
  title: string;
}

/** What analysing one track found, now written to the library. */
export interface AnalysisResult {
  trackId: string;
  bpmX100: number;
  key: string;
  beats: number;
  /** Peak sample magnitude, 0 to 1. */
  peak: number;
  durationSec: number;
  /** How long the analysis took, for the progress readout. */
  elapsedMs: number;
  /** Where the analysis files went, share-relative. */
  analysisPath: string;
}

/** What an import batch did. */
export interface ImportReport {
  imported: number;
  /** One line per file that was not imported, saying why. */
  skipped: string[];
  /** The tracks that landed, so they can be queued for analysis. */
  tracks: { id: string; title: string }[];
}

/** What importing a rekordbox XML collection did. */
export interface XmlImportReport {
  imported: number;
  /** Tracks whose file was already in the library, reused as they are. */
  existing: number;
  skipped: string[];
  playlists: number;
  cues: number;
  tracks: { id: string; title: string }[];
}

/** What an automatic relocate did. */
export interface RelocateReport {
  relocated: number;
  /** Missing tracks whose file name was found in none of the folders. */
  unresolved: number;
}

/** A track whose audio file is no longer where the library says. */
export interface MissingTrack {
  id: string;
  title: string;
  artist: string;
  /** Where the library still expects it. */
  path: string;
}

export interface DuplicateTrack {
  id: string;
  path: string;
  durationSec: number;
  /** Whether the file is where the library says. */
  present: boolean;
}

export interface DuplicateGroup {
  title: string;
  artist: string;
  tracks: DuplicateTrack[];
}

export interface Duplicates {
  /** Every group, not just the ones listed. */
  groups: number;
  /** Copies beyond the first, over every group. */
  extra: number;
  shown: DuplicateGroup[];
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
  /**
   * Moves a playlist or folder under `parent`.
   *
   * `index` is the place to take among that parent's children, counted once
   * the node has been lifted out of wherever it was. Omitted, it is appended.
   */
  movePlaylist(id: string, parent: string, index?: number): Promise<number>;
  deletePlaylist(id: string): Promise<number>;
  addTracksToPlaylist(playlist: string, tracks: string[]): Promise<number>;
  removeTracksFromPlaylist(playlist: string, tracks: string[]): Promise<number>;
  /** Reset DJ Play Count: back to zero on each track. */
  resetPlayCount(tracks: string[]): Promise<number>;
  /** A play: the track goes on today's history session and its count goes up. */
  recordPlay(track: string): Promise<number>;
  /** Remove from History: the tracks' plays leave the session. */
  removeFromHistory(history: string, tracks: string[]): Promise<number>;
  /** Remove from Collection: the tracks leave the library and every playlist. The files stay. */
  removeFromCollection(tracks: string[]): Promise<number>;
  reorderPlaylist(playlist: string, tracks: string[]): Promise<number>;
  setTrackRating(track: string, stars: number): Promise<number>;
  setTrackComment(track: string, comment: string): Promise<number>;
  setTrackColor(track: string, color: string | null): Promise<number>;
  /**
   * One of the Info tab's editable fields, by wire name. The backend keeps
   * the list of what may be written; a name it does not know is refused as
   * `readOnly` rather than mapped onto a guess.
   */
  setTrackField(track: string, field: TrackField, value: string): Promise<number>;
  /** Sets the My Tags on a track to exactly these ids. */
  setMyTags(track: string, tags: string[]): Promise<number>;
  /** Add Artwork: the image is filed in the share tree and the track points at it. */
  addArtwork(track: string, image: string): Promise<number>;
  /** Delete Artwork: the track points at no image; the file stays. */
  clearArtwork(track: string): Promise<number>;

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
  /**
   * Convert Memory Cues to Hot Cues: each memory cue, by position, into the
   * next free slot from A. Resolves to how many were made.
   */
  convertMemoryCuesToHot(track: string): Promise<number>;
}

/** One of the folders the Explorer starts from. */
export interface ExplorerRoot {
  /** What to show: `Music`, the user's name, `Macintosh HD`, a stick's name. */
  name: string;
  path: string;
}

/** The folders directly under one folder, by name. */
export interface ExplorerChildren {
  /** The first of them by name, up to the backend's cap. */
  names: string[];
  /** How many there were: more than `names` holds when the cap cut it. */
  total: number;
}

/** `ParentID` of a playlist or folder at the top of the tree. */
export const TREE_ROOT = "root";

/**
 * One track, in full — the information panel's record.
 *
 * Numbers the library leaves NULL arrive as 0 and text as empty, so nothing
 * here is optional; rekordbox's own Info tab prints 0 in an empty Year box.
 */
export interface TrackDetails {
  id: string;
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  originalArtist: string;
  composer: string;
  remixer: string;
  lyricist: string;
  genre: string;
  label: string;
  key: string;
  comment: string;
  mixName: string;
  message: string;
  /** `"0"` or empty for none, `"1"` to `"8"` for rekordbox's eight colours. */
  color: string;
  rating: number;
  bpmX100: number;
  durationSec: number;
  year: number;
  trackNumber: number;
  discNumber: number;
  playCount: number;
  /** rekordbox's own code: 1 MP3, 4 M4A, 5 FLAC, 11 WAV, 12 AIFF. */
  fileType: number;
  fileSize: number;
  /** kbps. */
  bitrate: number;
  /** Hz. */
  sampleRate: number;
  bitDepth: number;
  /** `YYYY-MM-DD`. */
  dateCreated: string;
  releaseDate: string;
  /** The audio file's absolute path. */
  path: string;
  hotCueAutoLoad: boolean;
  publish: boolean;
  hasArtwork: boolean;
  /** The ids of the My Tags on the track. */
  myTags: string[];
}

export interface TrackLookups {
  keys: string[];
  genres: string[];
  /** The library's My Tags by category, with the ids the toggles set. */
  myTagCategories: { name: string; tags: { id: string; name: string }[] }[];
}

/** The fields the backend will write. Everything else on the Info tab is shown read-only. */
export type TrackField =
  | "title" | "artist" | "album" | "year" | "trackNumber" | "discNumber"
  | "originalArtist" | "composer" | "remixer" | "lyricist" | "playCount"
  | "genre" | "label" | "key" | "bpm";

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

/** The reference rows a fresh stick's `exportLibrary.db` starts from. */
export interface ReferenceStickSettings {
  categories: MenuSlot[];
  sorts: MenuSlot[];
}

/**
 * What a stick gets on its first export: the Preferences window's DJ System
 * pane. `categories` and `sorts` replace the reference rows; `subColumn`
 * is the sort option shown beside the track name, or null for none.
 */
export interface StickDefaults {
  waveformColor: WaveformColor;
  waveformPosition: WaveformPosition;
  overviewWaveform: OverviewWaveform;
  keyDisplay: KeyDisplay;
  categories: MenuSlot[] | null;
  sorts: MenuSlot[] | null;
  subColumn: number | null;
}

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
