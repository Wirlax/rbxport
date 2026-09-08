/**
 * Where a track's artwork comes from.
 *
 * A custom scheme rather than `invoke`: a JPEG is tens of kilobytes, which
 * blows the 64 KB IPC cap, and a base64 payload would cost a main-thread
 * decode for every row. An `<img src>` lets the webview fetch, decode and
 * cache it off the UI thread.
 *
 * Outside Tauri — `pnpm dev:mock` and the Playwright suite — there is no such
 * scheme, so this returns nothing and the tint stands in.
 */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function artworkUrl(trackId: string): string | undefined {
  if (!isTauri) return undefined;
  // The backend resolves the id against the library; it never takes a path
  // from here, so nothing in the webview can name a file to read.
  return `rbl://artwork/${encodeURIComponent(trackId)}`;
}
