/**
 * The only module that talks to Tauri.
 *
 * Views import the typed functions here; they never call `invoke` themselves,
 * so the IPC surface stays auditable and the mock can stand in wholesale.
 */
import type { Backend, LibrarySummary, RowDto, TreeNode, ViewHandle, WaveformKind } from "./types";

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
