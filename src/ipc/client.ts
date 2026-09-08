/**
 * The only module that talks to Tauri.
 *
 * Views import the typed functions here; they never call `invoke` themselves,
 * so the IPC surface stays auditable and the mock can stand in wholesale.
 */
import type {
  AnalysisResult, Backend, Beat, Cue, Device, ExportReport, ImportReport, LibrarySummary, LinkPeer,
  LinkStatus, MissingTracks, RowDto,
  TreeNode, ViewHandle,
  WaveformKind,
} from "./types";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function realBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    librarySummary: () => invoke<LibrarySummary>("library_summary"),
    playlistTree: () => invoke<TreeNode[]>("playlist_tree"),
    openView: (spec) => invoke<ViewHandle>("open_view", { spec }),
    fetchRows: (viewId, offset, len) => invoke<RowDto[]>("fetch_rows", { viewId, offset, len }),
    viewIdsInRange: (viewId, from, to) => invoke<string[]>("view_ids_in_range", { viewId, from, to }),
    trackWaveform: async (trackId: string, kind: WaveformKind) => {
      // Tauri hands a Rust Vec<u8> back as a number array; normalise here so
      // views only ever see a typed array.
      const bytes = await invoke<number[] | Uint8Array>("track_waveform", { trackId, kind });
      return bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
    },
    analyseTrack: (trackId) => invoke<AnalysisResult>("analyse_track", { trackId }),
    trackBeats: (trackId, fromMs, toMs) =>
      invoke<Beat[]>("track_beats", { track: trackId, fromMs, toMs }),
    trackCues: (trackId) => invoke<Cue[]>("track_cues", { track: trackId }),
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
    onMenu: (listener) => {
      let live = true;
      let stop: (() => void) | undefined;
      void import("@tauri-apps/api/event").then(async ({ listen }) => {
        const unlisten = await listen<string>("menu", (event) => listener(event.payload));
        // The caller may have unsubscribed while the import was in flight.
        if (live) stop = unlisten;
        else unlisten();
      });
      return () => {
        live = false;
        stop?.();
      };
    },
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
