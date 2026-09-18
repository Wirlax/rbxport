/**
 * The player's waveform: a window around the playhead.
 *
 * A `span` of 1 is the whole track, which is what the overview strip above the
 * detail wants — same component, same bytes, one fetch.
 *
 * Deliberately not the row preview's component. That one renders once per
 * track and caches a bitmap, which is right for a row that never changes and
 * wrong here — the window moves with playback, so a cached bitmap would be
 * stale on the next frame. This draws straight from the bytes instead, which
 * are a few hundred of them and cost nothing to redraw.
 */
import { memo, useEffect, useLayoutEffect, useRef, useState } from "react";

import { drawWave, strideOf, waveformKindOf, type HalfWaveform } from "@/canvas";
import { getBackend } from "@/ipc/client";
import type { WaveformKind } from "@/ipc/types";
import { backingSize } from "@/lib/canvasSize";
import { waveSlice } from "@/lib/player";
import { usePreferences } from "@/store/usePreferences";

/** Raw waveform bytes per track. Small — a few hundred bytes each. */
const bytesByTrack = new Map<string, Uint8Array>();
/** In-flight fetches, shared so StrictMode's double effect does not double-fetch. */
const inFlight = new Map<string, Promise<Uint8Array>>();

async function load(trackId: string, kind: WaveformKind): Promise<Uint8Array> {
  const key = `${trackId}:${kind}`;
  const cached = bytesByTrack.get(key);
  if (cached) return cached;
  const existing = inFlight.get(key);
  if (existing) return existing;

  const pending = (async () => {
    try {
      const backend = await getBackend();
      const data = await backend.trackWaveform(trackId, kind);
      bytesByTrack.set(key, data);
      return data;
    } catch {
      // A track without analysis simply stays blank.
      return new Uint8Array();
    } finally {
      inFlight.delete(key);
    }
  })();
  inFlight.set(key, pending);
  return pending;
}

export interface WaveformDetailProps {
  trackId: string;
  /** How far through the track playback is, 0 to 1. */
  progress: number;
  /** How much of the track the window spans, as a fraction. 1 is all of it. */
  span?: number;
  /** The space the canvas occupies, in CSS pixels. */
  width: number;
  height: number;
  /** Half height from the baseline: stacked for the overview, overlaid for the 2 PLAYER detail. */
  half?: HalfWaveform;
  /** Read the full-resolution `PWV7` rather than the 1,200-column `PWV6`. */
  detail?: boolean;
  /** Rows to leave clear at the top and bottom, in CSS pixels. */
  inset?: { top: number; bottom: number };
}

/** Drops a track's bytes: its analysis was rewritten. */
function forget(trackId: string): void {
  for (const key of [...bytesByTrack.keys()]) {
    if (key.startsWith(`${trackId}:`)) bytesByTrack.delete(key);
  }
}

export const WaveformDetail = memo(function WaveformDetail({
  trackId, progress, span = 0.08, width, height, half = false, detail = false,
  inset = { top: 0, bottom: 0 },
}: WaveformDetailProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<Uint8Array | null>(null);
  // Bumped when the track is re-analysed, so the bytes are fetched again.
  const [revision, setRevision] = useState(0);
  // View › Color › Waveform color: each palette reads its own tags.
  const palette = usePreferences().view.waveformColor;

  useEffect(() => {
    let live = true;
    setData(null);
    void load(trackId, waveformKindOf(palette, detail)).then((bytes) => {
      if (live) setData(bytes);
    });
    return () => {
      live = false;
    };
  }, [trackId, detail, palette, revision]);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      if (!live) return;
      stop = backend.onAnalysisChanged((changed) => {
        if (changed !== trackId) return;
        forget(trackId);
        setRevision((r) => r + 1);
      });
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, [trackId]);

  // A layout effect, so the redraw lands before the frame that shows it. The
  // strip that holds this is slid by a transform written in the parent's own
  // layout effect, and children run first: the two stay in step only because
  // the canvas is already new by the time the slide is written.
  useLayoutEffect(() => {
    const canvas = ref.current;
    if (!canvas || !data) return;
    const { width: w, height: h } = backingSize(width, height);
    // Assigning either clears the canvas, so only when it actually changed.
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Centred on the playhead and *not* pinned: the head stays in the middle
    // and the track moves under it, so at either end the window hangs off the
    // edge. The overhang is drawn as nothing rather than as a stretched copy
    // of the first bar. Both tags cover the whole track, so a window into one
    // is a slice of its columns rather than a second fetch.
    const { first, last, x0, width: span_ } = waveSlice(progress, span, data.length, w, strideOf(palette, detail));
    // The inset is given in CSS pixels; the canvas is in device pixels.
    const scale = h / Math.max(height, 1);
    ctx.clearRect(0, 0, w, h);
    if (last <= first || span_ < 1) return;
    ctx.save();
    ctx.translate(x0, 0);
    drawWave(ctx, data.subarray(first, last), span_, h, palette, detail, half, {
      top: inset.top * scale,
      bottom: inset.bottom * scale,
    });
    ctx.restore();
  }, [data, progress, span, width, height, half, detail, palette, inset.top, inset.bottom]);

  return <canvas ref={ref} style={{ width: "100%", height: "100%", display: "block" }} />;
});
