/**
 * The Preferences as a window of its own.
 *
 * The shell opens a second webview at `index.html#preferences/<pane>`, and
 * this is what it renders: the same Preferences the browser build draws over
 * itself, filling a window the platform gives a title bar to drag by. Its
 * choices go to the storage both windows share; what only the main window
 * holds — the columns, the panes' widths — is asked for over an event.
 */
import { useCallback, useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { LibrarySummary } from "@/ipc/types";
import { useLimiter } from "@/store/useLimiter";
import { useMaster } from "@/store/useMaster";
import { PreferencesProvider, usePreferencesStore } from "@/store/usePreferences";
import { asPane, Preferences, type Pane } from "./Preferences";

/** The pane the hash names: `#preferences/advanced` is Advanced. */
export function paneFromHash(hash: string): Pane {
  return asPane(hash.replace(/^#preferences\/?/, "").split(/[/?]/)[0]);
}

export function PreferencesWindow() {
  const store = usePreferencesStore();
  const [pane, setPane] = useState<Pane>(() => paneFromHash(window.location.hash));
  const [summary, setSummary] = useState<LibrarySummary | null>(null);
  // The limiter is the engine's, so this window reads and sets it the same
  // way the shell does, and its meter ticks arrive here as they do there.
  const limiter = useLimiter();
  const master = useMaster();

  // Turned to another pane by the shell while open: it sets the hash.
  useEffect(() => {
    const onHash = () => setPane(paneFromHash(window.location.hash));
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // The library's facts, for Advanced › Database; read here, since this
  // window has no shell of its own holding them.
  useEffect(() => {
    let live = true;
    void getBackend()
      .then((backend) => backend.librarySummary())
      .then((read) => {
        if (live) setSummary(read);
      })
      .catch(() => {
        // Not up yet: the facts show dashes, as the main window's would.
      });
    return () => {
      live = false;
    };
  }, []);

  const close = useCallback(() => {
    void getBackend().then((backend) => backend.closeWindow());
  }, []);
  const ask = useCallback((what: "columns" | "layout") => {
    void getBackend().then((backend) => backend.requestPreferencesReset(what));
  }, []);

  return (
    <PreferencesProvider value={store}>
      <Preferences
        windowed
        summary={summary}
        limiter={limiter.limiter}
        onLimiterChange={limiter.set}
        reduction={master.reduction}
        initialPane={pane}
        onResetColumns={() => ask("columns")}
        onResetLayout={() => ask("layout")}
        onClose={close}
      />
    </PreferencesProvider>
  );
}
