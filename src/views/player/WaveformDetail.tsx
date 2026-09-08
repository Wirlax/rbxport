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
import { memo, useEffect, useRef, useState } from "react";

import { drawPreview } from "@/canvas";
import { getBackend } from "@/ipc/client";
import { backingSize } from "@/lib/canvasSize";

/** Raw waveform bytes per track. Small — a few hundred bytes each. */
const bytesByTrack = new Map<string, Uint8Array>();
/** In-flight fetches, shared so StrictMode's double effect does not double-fetch. */
const inFlight = new Map<string, Promise<Uint8Array>>();

async function load(trackId: string): Promise<Uint8Array> {
  const cached = bytesByTrack.get(trackId);
  if (cached) return cached;
  const existing = inFlight.get(trackId);
  if (existing) return existing;

  const pending = (async () => {
    try {
      const backend = await getBackend();
      const data = await backend.trackWaveform(trackId, "preview");
      bytesByTrack.set(trackId, data);
      return data;
    } catch {
      // A track without analysis simply stays blank.
      return new Uint8Array();
    } finally {
      inFlight.delete(trackId);
    }
  })();
  inFlight.set(trackId, pending);
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
}

export const WaveformDetail = memo(function WaveformDetail({
  trackId, progress, span = 0.08, width, height,
}: WaveformDetailProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<Uint8Array | null>(null);

  useEffect(() => {
    let live = true;
    setData(null);
    void load(trackId).then((bytes) => {
      if (live) setData(bytes);
    });
    return () => {
      live = false;
    };
  }, [trackId]);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || !data) return;
    const { width: w, height: h } = backingSize(width, height);
    // Assigning either clears the canvas, so only when it actually changed.
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Centred on the playhead, and pinned at either end so the window never
    // runs off the track and leaves half the panel empty.
    const half = span / 2;
    const centre = Math.min(Math.max(progress, half), 1 - half);
    // The detail's amber is the darker of the two; the overview strip above it
    // uses the brighter one.
    drawPreview(ctx, data, w, h, {
      window: { from: centre - half, to: centre + half },
      band: span >= 1 ? "overview" : "detail",
    });
  }, [data, progress, span, width, height]);

  return <canvas ref={ref} style={{ width: "100%", height: "100%", display: "block" }} />;
});
