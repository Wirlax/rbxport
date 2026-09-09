/**
 * Whether this build can play anything.
 *
 * Playback is the Rust engine behind the `deck_*` commands, so it exists only
 * inside Tauri. In a browser — `pnpm dev:mock`, and Playwright — the transport
 * is drawn and disabled: a player waiting for a backend, rather than an
 * unfinished panel.
 */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const canPlay = isTauri;
