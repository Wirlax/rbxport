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

import { drawBands } from "@/canvas";
import { getBackend } from "@/ipc/client";
import { backingSize } from "@/lib/canvasSize";

/** Raw waveform bytes per track. Small — a few hundred bytes each. */
const bytesByTrack = new Map<string, Uint8Array>();
/** In-flight fetches, shared so StrictMode's double effect does not double-fetch. */
const inFlight = new Map<string, Promise<Uint8Array>>();

async function load(trackId: string, kind: "bands" | "bandsDetail"): Promise<Uint8Array> {
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
  /** Half height from the baseline, bands stacked — the overview's form. */
  half?: boolean;
  /** Read the full-resolution `PWV7` rather than the 1,200-column `PWV6`. */
  detail?: boolean;
}

export const WaveformDetail = memo(function WaveformDetail({
  trackId, progress, span = 0.08, width, height, half = false, detail = false,
}: WaveformDetailProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [data, setData] = useState<Uint8Array | null>(null);

  useEffect(() => {
    let live = true;
    setData(null);
    void load(trackId, detail ? "bandsDetail" : "bands").then((bytes) => {
      if (live) setData(bytes);
    });
    return () => {
      live = false;
    };
  }, [trackId, detail]);

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
    const reach = Math.min(span, 1) / 2;
    const centre = Math.min(Math.max(progress, reach), 1 - reach);
    // The detail's amber is the darker of the two; the overview strip above it
    // uses the brighter one.
    // Both tags cover the whole track, so a window into one is a slice of its
    // columns rather than a second fetch.
    const columns = Math.floor(data.length / 3);
    const first = Math.max(0, Math.floor((centre - reach) * columns)) * 3;
    const last = Math.min(data.length, Math.ceil((centre + reach) * columns) * 3);
    drawBands(ctx, data.subarray(first, last), w, h, detail ? "detail" : "overview", half);
  }, [data, progress, span, width, height, half, detail]);

  return <canvas ref={ref} style={{ width: "100%", height: "100%", display: "block" }} />;
});
