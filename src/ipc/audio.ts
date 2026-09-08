/**
 * Where a track's audio comes from.
 *
 * The same scheme as artwork, and for the same reason: an `<audio src>` lets
 * the webview stream, decode and buffer off the UI thread, where pulling
 * bytes through `invoke` would blow the IPC cap and block the main thread.
 * The backend answers range requests, so seeking costs one chunk rather than
 * a whole file.
 *
 * Outside Tauri there is no such scheme, so this returns nothing and the
 * player stays inert — which is what `pnpm dev:mock` and Playwright see.
 */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function audioUrl(trackId: string): string | undefined {
  if (!isTauri) return undefined;
  // Resolved against the library by id; no path crosses from here.
  return `rbl://audio/${encodeURIComponent(trackId)}`;
}

/** Whether this build can play anything at all. */
export const canPlay = isTauri;
