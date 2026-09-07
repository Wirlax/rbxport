/**
 * One row's waveform.
 *
 * Fetching and rendering happen once per track and the result is cached as a
 * bitmap, so scrolling back over a row is a single `drawImage` rather than an
 * IPC round trip and a redraw.
 */
import { memo, useEffect, useRef } from "react";
import { getBackend } from "@/ipc/client";
import { renderPreview, WaveformCache, type RenderedWaveform } from "@/canvas";

/** Shared across every row: bounded, and released when entries fall out. */
const cache = new WaveformCache(500);

/**
 * In-flight renders, shared rather than skipped.
 *
 * An earlier version kept a Set and returned early when a key was already in
 * flight. Under StrictMode the effect runs twice: the first pass cancels itself
 * on cleanup and the second sees the in-flight marker and returns, so nothing
 * ever painted. Sharing the promise means every caller still gets the result.
 */
const inFlight = new Map<string, Promise<RenderedWaveform | null>>();

async function load(
  trackId: string,
  key: string,
  width: number,
  height: number,
  dpr: number,
): Promise<RenderedWaveform | null> {
  const existing = inFlight.get(key);
  if (existing) return existing;

  const pending = (async () => {
    try {
      const backend = await getBackend();
      const data = await backend.trackWaveform(trackId, "preview");
      if (data.length === 0) return null;
      const rendered = await renderPreview(data, width, height, dpr);
      if (rendered) cache.set(key, rendered);
      return rendered;
    } catch {
      // A track without analysis simply stays blank.
      return null;
    } finally {
      inFlight.delete(key);
    }
  })();

  inFlight.set(key, pending);
  return pending;
}

export interface WaveformPreviewProps {
  trackId: string;
  width: number;
  height: number;
}

export const WaveformPreview = memo(function WaveformPreview({
  trackId,
  width,
  height,
}: WaveformPreviewProps) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    let cancelled = false;
    const dpr = window.devicePixelRatio || 1;
    const key = WaveformCache.key(trackId, width, dpr);

    const paint = (entry: { bitmap: CanvasImageSource }) => {
      const canvas = ref.current;
      if (!canvas || cancelled) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(entry.bitmap, 0, 0, canvas.width, canvas.height);
    };

    const cached = cache.get(key);
    if (cached) {
      paint(cached);
      return;
    }

    void load(trackId, key, width, height, dpr).then((rendered) => {
      if (rendered) paint(rendered);
    });

    return () => {
      cancelled = true;
    };
  }, [trackId, width, height]);

  const dpr = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1;
  return (
    <canvas
      ref={ref}
      width={Math.round(width * dpr)}
      height={Math.round(height * dpr)}
      style={{ width: `${width}px`, height: `${height}px` }}
      aria-hidden
    />
  );
});
