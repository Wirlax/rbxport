/**
 * What the app is costing, for the title bar's readout.
 *
 * A second between readings, chained with `setTimeout` rather than run on an
 * interval: an interval that outlives the page keeps firing, and a reading is
 * only worth taking once the last one has come back.
 *
 * Frames are counted here rather than asked for. The backend cannot see the
 * webview's refresh rate, and a permanent `requestAnimationFrame` loop is the
 * exact thing the idle budget rules out — so each reading runs the loop for a
 * short burst, times the frames it saw, and stops.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { Diagnostics } from "@/ipc/types";

/** Between readings. */
const PERIOD_MS = 1000;

/** Frames timed per reading. Enough to be steady, short enough to be idle. */
const FRAMES = 10;

export interface AppCost extends Diagnostics {
  /** Frames a second, measured in the webview, or `null` before the first. */
  fps: number | null;
}

const NOTHING: AppCost = {
  cpu: 0,
  memoryMb: 0,
  threads: null,
  openFiles: null,
  gpu: null,
  fps: null,
};

/** Times `FRAMES` frames and returns the rate they arrived at. */
function measureFps(): Promise<number> {
  return new Promise((resolve) => {
    const start = performance.now();
    let seen = 0;
    const step = () => {
      seen += 1;
      if (seen < FRAMES) {
        requestAnimationFrame(step);
        return;
      }
      const elapsed = performance.now() - start;
      resolve(elapsed > 0 ? (seen / elapsed) * 1000 : 0);
    };
    requestAnimationFrame(step);
  });
}

export function useDiagnostics(enabled: boolean): AppCost {
  const [cost, setCost] = useState<AppCost>(NOTHING);

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const read = async () => {
      try {
        const backend = await getBackend();
        const [sample, fps] = await Promise.all([backend.appDiagnostics(), measureFps()]);
        if (live) setCost({ ...sample, fps });
      } catch {
        // A build with no backend behind it simply shows nothing.
        if (live) setCost(NOTHING);
      }
      if (live) timer = setTimeout(() => void read(), PERIOD_MS);
    };
    void read();

    return () => {
      live = false;
      if (timer !== null) clearTimeout(timer);
    };
  }, [enabled]);

  return cost;
}

/** `1.2 GB`, `312 MB` — the readout has no room for six digits of bytes. */
export function formatMemory(mb: number): string {
  if (!Number.isFinite(mb) || mb <= 0) return "—";
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

/** A whole number, or a dash where the platform will not say. */
export function formatCount(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : String(Math.round(value));
}

/** `12%`, or a dash. */
export function formatPercent(value: number | null): string {
  return value === null || !Number.isFinite(value) ? "—" : `${Math.round(value)}%`;
}
