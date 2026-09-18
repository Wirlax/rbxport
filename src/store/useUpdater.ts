/**
 * Software updates, as the Update Manager window sees them.
 *
 * One state machine: idle → checking → up to date | available → downloading
 * → installing, with failed reachable from any of the working states. The
 * backend does the checking, downloading and installing; this holds where it
 * has got to and whether the window is showing.
 *
 * A check the app starts on its own only opens the window when there is
 * something to offer: nobody wants "you're up to date" every morning. A
 * check somebody asked for opens it at once and reports either way.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { UpdateFrequency } from "@/lib/preferences";
import type { UpdateCheck, UpdateProgress } from "@/ipc/types";

/** How long after launch the automatic check runs: after the library, not before it. */
const AUTO_CHECK_AFTER_MS = 15_000;

export type UpdaterState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate"; currentVersion: string }
  | { phase: "available"; check: UpdateCheck }
  | { phase: "downloading"; check: UpdateCheck; progress: UpdateProgress | null }
  | { phase: "installing"; check: UpdateCheck }
  | { phase: "failed"; message: string; check: UpdateCheck | null };

export interface Updater {
  state: UpdaterState;
  /** Whether the Update Manager window is showing. */
  open: boolean;
  /** Ask the server. `manual` opens the window whatever the answer. */
  check: (manual: boolean) => void;
  /** Download and install what the last check found. */
  install: () => void;
  /** Close the window; a download in progress keeps going. */
  dismiss: () => void;
}

/** Why an install that returned failed, in the words the window shows. */
function reason(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message !== "") return message;
  }
  if (typeof error === "string" && error !== "") return error;
  return "An error occurred. Please try later.";
}

/** Where the last automatic check's time is kept, so the frequency holds across launches. */
const LAST_CHECK_KEY = "rbxport.updates.lastCheck";

/** The shortest gap between automatic checks for each frequency, in milliseconds. */
const CHECK_GAP_MS: Record<UpdateFrequency, number> = {
  start: 0,
  daily: 24 * 60 * 60 * 1000,
  weekly: 7 * 24 * 60 * 60 * 1000,
};

/** Whether an automatic check is due, by the last one's time. */
export function checkDue(frequency: UpdateFrequency, lastCheckMs: number | null, nowMs: number): boolean {
  if (frequency === "start" || lastCheckMs === null || !Number.isFinite(lastCheckMs)) return true;
  return nowMs - lastCheckMs >= CHECK_GAP_MS[frequency];
}

function lastCheck(): number | null {
  try {
    const stored = localStorage.getItem(LAST_CHECK_KEY);
    return stored === null ? null : Number.parseInt(stored, 10);
  } catch {
    return null;
  }
}

function noteCheck(nowMs: number): void {
  try {
    localStorage.setItem(LAST_CHECK_KEY, String(nowMs));
  } catch {
    // Storage refused: the next launch checks again, which is the safe side.
  }
}

export function useUpdater(autoCheck: boolean, frequency: UpdateFrequency = "start"): Updater {
  const [state, setState] = useState<UpdaterState>({ phase: "idle" });
  const [open, setOpen] = useState(false);
  // The state outside a render, so an action can read it without a side
  // effect inside a state updater.
  const latest = useRef(state);
  latest.current = state;
  // The check that is running, so a slow one does not overwrite a newer one.
  const sequence = useRef(0);
  const checkedOnStart = useRef(false);

  const check = useCallback((manual: boolean) => {
    const mine = ++sequence.current;
    setState({ phase: "checking" });
    if (manual) setOpen(true);
    void (async () => {
      try {
        const backend = await getBackend();
        const found = await backend.checkForUpdate();
        if (mine !== sequence.current) return;
        if (found.version === null) {
          setState({ phase: "upToDate", currentVersion: found.currentVersion });
        } else {
          setState({ phase: "available", check: found });
          setOpen(true);
        }
      } catch (error) {
        if (mine !== sequence.current) return;
        setState({ phase: "failed", message: reason(error), check: null });
        // A check the app ran on its own that could not reach the server is
        // not worth a window: the next launch tries again.
        if (manual) setOpen(true);
      }
    })();
  }, []);

  const install = useCallback(() => {
    const current = latest.current;
    if (current.phase !== "available" && current.phase !== "failed") return;
    const found = current.check;
    if (!found) return;
    setState({ phase: "downloading", check: found, progress: null });
    void (async () => {
      try {
        const backend = await getBackend();
        // A success never returns: the app restarts. Returning is failure.
        await backend.installUpdate();
        setState({ phase: "failed", message: "The update did not restart the app.", check: found });
      } catch (error) {
        setState({ phase: "failed", message: reason(error), check: found });
      }
    })();
  }, []);

  // The download's progress, and the moment it turns into an install.
  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void getBackend().then((backend) => {
      if (!live) return;
      stop = backend.onUpdateProgress((progress) => {
        setState((current) => {
          if (current.phase !== "downloading") return current;
          const done = progress.total !== null && progress.downloaded >= progress.total;
          return done
            ? { phase: "installing", check: current.check }
            : { ...current, progress };
        });
      });
    });
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  useEffect(() => {
    if (!autoCheck || checkedOnStart.current) return;
    checkedOnStart.current = true;
    if (!checkDue(frequency, lastCheck(), Date.now())) return;
    const timer = setTimeout(() => {
      noteCheck(Date.now());
      check(false);
    }, AUTO_CHECK_AFTER_MS);
    return () => clearTimeout(timer);
  }, [autoCheck, frequency, check]);

  const dismiss = useCallback(() => setOpen(false), []);

  return { state, open, check, install, dismiss };
}
