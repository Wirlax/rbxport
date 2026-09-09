/**
 * The only module that talks to Tauri.
 *
 * Views import the typed functions here; they never call `invoke` themselves,
 * so the IPC surface stays auditable and the mock can stand in wholesale.
 */
import type {
  AnalysisResult, AudioDevices, Backend, Cue, DeckEvent, Device, Diagnostics, ExportReport,
  Phrase, ImportReport,
  LibrarySummary, LinkPeer, Meters,
  LinkStatus, MissingTracks, RowDto, Tick,
  TreeNode, ViewHandle,
} from "./types";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Subscribes to a backend event, returning its own unsubscribe.
 *
 * The event module is imported lazily, so a caller can unsubscribe before the
 * import lands; that case has to be handled or the listener outlives its
 * component.
 */
function subscribe<T>(event: string, listener: (payload: T) => void): () => void {
  let live = true;
  let stop: (() => void) | undefined;
  void import("@tauri-apps/api/event").then(async ({ listen }) => {
    const unlisten = await listen<T>(event, (e) => listener(e.payload));
    if (live) stop = unlisten;
    else unlisten();
  });
  return () => {
    live = false;
    stop?.();
  };
}

async function realBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    librarySummary: () => invoke<LibrarySummary>("library_summary"),
    playlistTree: () => invoke<TreeNode[]>("playlist_tree"),
    openView: (spec) => invoke<ViewHandle>("open_view", { spec }),
    fetchRows: (viewId, offset, len) => invoke<RowDto[]>("fetch_rows", { viewId, offset, len }),
    viewIdsInRange: (viewId, from, to) => invoke<string[]>("view_ids_in_range", { viewId, from, to }),
    trackWaveform: async (trackId, kind, window) => {
      // Raw bytes rather than a JSON number array: the three-band detail tag
      // is 158 KB on a five-minute track and JSON would multiply that.
      const bytes = await invoke<ArrayBuffer | number[] | Uint8Array>("track_waveform", {
        trackId,
        kind,
        from: window?.from,
        len: window?.len,
      });
      if (bytes instanceof ArrayBuffer) return new Uint8Array(bytes);
      return bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
    },
    analyseTrack: (trackId) => invoke<AnalysisResult>("analyse_track", { trackId }),
    trackBeats: async (trackId) => {
      // Raw bytes rather than a JSON array of objects: a long mix has tens of
      // thousands of beats, and `{"timeMs":123,"downbeat":true}` each is an
      // order of magnitude more to send and to parse.
      const bytes = await invoke<ArrayBuffer | number[] | Uint8Array>("track_beats", {
        track: trackId,
      });
      if (bytes instanceof ArrayBuffer) return new Uint8Array(bytes);
      return bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
    },
    trackCues: (trackId) => invoke<Cue[]>("track_cues", { track: trackId }),
    trackPhrases: (trackId) => invoke<Phrase[]>("track_phrases", { track: trackId }),
    trackVocals: async (trackId) => {
      const bytes = await invoke<ArrayBuffer | number[] | Uint8Array>("track_vocals", {
        track: trackId,
      });
      if (bytes instanceof ArrayBuffer) return new Uint8Array(bytes);
      return bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
    },
    importFiles: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: true,
        directory: false,
        title: "Add music to the library",
        filters: [
          {
            name: "Audio",
            extensions: ["mp3", "m4a", "aac", "flac", "wav", "aiff", "aif", "ogg", "opus"],
          },
        ],
      });
      // Cancelling is a normal outcome, not an error.
      if (!Array.isArray(picked) || picked.length === 0) return null;
      return invoke<ImportReport>("import_files", { paths: picked });
    },
    exportPlaylist: async (playlistId, destination) => {
      let target = destination;
      if (target === undefined) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const picked = await open({
          multiple: false,
          directory: true,
          title: "Choose where to write the export",
        });
        // Cancelling is a normal outcome, not an error.
        if (typeof picked !== "string") return null;
        target = picked;
      }
      return invoke<ExportReport>("export_playlist", {
        playlist: playlistId,
        destination: target,
      });
    },
    listDevices: () => invoke<Device[]>("list_devices"),
    deckLoad: (deck, trackId) => invoke<void>("deck_load", { deck, track: trackId }),
    deckUnload: (deck) => invoke<void>("deck_unload", { deck }),
    deckPlay: (deck) => invoke<void>("deck_play", { deck }),
    deckPause: (deck) => invoke<void>("deck_pause", { deck }),
    deckSeek: (deck, positionMs) => invoke<void>("deck_seek", { deck, positionMs }),
    deckScrubBegin: (deck) => invoke<void>("deck_scrub_begin", { deck }),
    deckScrubTo: (deck, positionMs) => invoke<void>("deck_scrub_to", { deck, positionMs }),
    deckScrubEnd: (deck) => invoke<void>("deck_scrub_end", { deck }),
    setMasterLevel: (level) => invoke<void>("set_master_level", { level }),
    audioDevices: () => invoke<AudioDevices>("audio_devices"),
    setAudioDevice: (device) => invoke<void>("set_audio_device", { device }),
    deckTempo: (deck, tempo) => invoke<void>("deck_tempo", { deck, tempo }),
    deckMasterTempo: (deck, on) => invoke<void>("deck_master_tempo", { deck, on }),
    setChannelBand: (deck, band, position) =>
      invoke<void>("set_channel_band", { deck, band, position }),
    setChannelKill: (deck, band, killed) =>
      invoke<void>("set_channel_kill", { deck, band, killed }),
    setChannelTrim: (deck, trim) => invoke<void>("set_channel_trim", { deck, trim }),
    setCrossfade: (position) => invoke<void>("set_crossfade", { position }),
    setEqCurve: (isolator) => invoke<void>("set_eq_curve", { isolator }),
    appDiagnostics: () => invoke<Diagnostics>("app_diagnostics"),
    revealTrack: (trackId) => invoke<void>("reveal_track", { track: trackId }),
    deckState: () => invoke<Tick>("deck_state"),
    onDeckTick: (listener) => subscribe<Tick>("deck:tick", listener),
    onMeters: (listener) => subscribe<Meters>("deck:meters", listener),
    onDeckEvent: (listener) => {
      // Loaded and failed are the same shape and the same subscription; the
      // message is what tells them apart.
      const stopLoaded = subscribe<DeckEvent>("deck:loaded", listener);
      const stopError = subscribe<DeckEvent>("deck:error", listener);
      return () => {
        stopLoaded();
        stopError();
      };
    },
    onLibraryReady: (listener) => subscribe("library:ready", () => listener()),
    onLibraryError: (listener) => subscribe<string>("library:error", listener),
    onMenu: (listener) => subscribe<string>("menu", listener),
    startLinkListening: () => invoke<LinkStatus>("start_link_listening"),
    stopLinkListening: () => invoke<void>("stop_link_listening"),
    onLinkPeers: (listener) => {
      let live = true;
      let stop: (() => void) | undefined;
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        const unlisten = await listen<LinkPeer[]>("link:peers", (e) => listener(e.payload));
        if (live) stop = unlisten;
        else unlisten();
      });
      return () => {
        live = false;
        stop?.();
      };
    },
    missingTracks: (limit) => invoke<MissingTracks>("missing_tracks", { limit }),
    relocateTrack: async (trackId) => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        directory: false,
        title: "Choose the file for this track",
        filters: [
          {
            name: "Audio",
            extensions: ["mp3", "m4a", "aiff", "aif", "wav", "flac", "aac", "ogg"],
          },
        ],
      });
      // Cancelling is a normal outcome, not an error.
      if (typeof picked !== "string") return null;
      await invoke<number>("relocate_track", { track: trackId, path: picked });
      return picked;
    },
    onLibraryChanged: (listener) => {
      // Tauri's listen resolves asynchronously; unsubscribing before it does
      // has to still work, so the flag is checked when it lands.
      let live = true;
      let stop: (() => void) | undefined;
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        const unlisten = await listen<number>("library:changed", (e) => listener(e.payload));
        if (live) stop = unlisten;
        else unlisten();
      });
      return () => {
        live = false;
        stop?.();
      };
    },
    edits: {
      createPlaylist: (name, parent) => invoke<number>("create_playlist", { name, parent }),
      createFolder: (name, parent) => invoke<number>("create_folder", { name, parent }),
      renamePlaylist: (id, name) => invoke<number>("rename_playlist", { id, name }),
      movePlaylist: (id, parent) => invoke<number>("move_playlist", { id, parent }),
      deletePlaylist: (id) => invoke<number>("delete_playlist", { id }),
      addTracksToPlaylist: (playlist, tracks) =>
        invoke<number>("add_tracks_to_playlist", { playlist, tracks }),
      removeTracksFromPlaylist: (playlist, tracks) =>
        invoke<number>("remove_tracks_from_playlist", { playlist, tracks }),
      reorderPlaylist: (playlist, tracks) =>
        invoke<number>("reorder_playlist", { playlist, tracks }),
      setTrackRating: (track, stars) => invoke<number>("set_track_rating", { track, stars }),
      setTrackComment: (track, comment) => invoke<number>("set_track_comment", { track, comment }),
      setTrackColor: (track, color) => invoke<number>("set_track_color", { track, color }),
    },
  };
}

let backendPromise: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  backendPromise ??= isTauri
    ? realBackend()
    : import("./backend-mock").then((m) => m.createMockBackend());
  return backendPromise;
}

/** Test seam: lets e2e and unit tests substitute a backend. */
export function __setBackend(b: Backend | null): void {
  backendPromise = b ? Promise.resolve(b) : null;
}
