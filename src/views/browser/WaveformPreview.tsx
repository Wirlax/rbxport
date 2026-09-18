/**
 * One row's waveform.
 *
 * Fetching and rendering happen once per track and the result is cached as a
 * bitmap, so scrolling back over a row is a single `drawImage` rather than an
 * IPC round trip and a redraw.
 */
import { memo, useEffect, useRef } from "react";
import { getBackend } from "@/ipc/client";
import type { RowCue } from "@/ipc/types";
import {
  drawPreviewCues, renderPreview, waveformKindOf, WaveformCache, type RenderedWaveform, type WavePalette,
} from "@/canvas";
import { usePreferences } from "@/store/usePreferences";

/** Shared across every row: bounded, and released when entries fall out. */
const cache = new WaveformCache(500);
// A re-analysed track's renderings are stale; its rows draw afresh when they
// next settle. One subscription for the module rather than one per row.
void getBackend().then((backend) => {
  backend.onAnalysisChanged((trackId) => cache.forget(trackId));
});

/**
 * In-flight renders, shared rather than skipped.
 *
 * An earlier version kept a Set and returned early when a key was already in
 * flight. Under StrictMode the effect runs twice: the first pass cancels itself
 * on cleanup and the second sees the in-flight marker and returns, so nothing
 * ever painted. Sharing the promise means every caller still gets the result.
 */
const inFlight = new Map<string, Promise<RenderedWaveform | null>>();

/**
 * How long a row must stay on screen before its waveform is asked for.
 *
 * A flick through a big playlist mounts and unmounts thousands of rows, and
 * every one of them used to cost a `track_waveform` round trip that read an
 * analysis file off disk — for a row nobody saw. Those run on the same
 * blocking pool as `fetch_rows`, so the rows being scrolled *to* queued behind
 * the waveforms of rows already gone, and the list came up blank until it
 * drained. About two frames: enough to still skip a hard flick (which turns a
 * page faster than this), short enough that a steady scroll starts loading a
 * row's waveform almost as soon as it appears.
 */
const SETTLE_MS = 35;

/**
 * How many waveform requests may be on the command channel at once.
 *
 * A screenful is about twenty rows and they all settle together. The cap keeps
 * them from putting twenty file reads in front of the next `fetch_rows`, which
 * is the call that has to land for the list to draw. It can afford to be wider
 * than it was: the row data is now fetched a window ahead (`PREFETCH_MARGIN`),
 * so a scroll rarely waits on `fetch_rows` at all, and the per-row scan that
 * made each waveform expensive is gone (`row_of_id`, a map lookup). Sixteen
 * fills a screen fast without the list text falling behind — measured on a
 * fresh scroll of the reference library, the text kept up.
 */
const MAX_CONCURRENT = 16;

let active = 0;
const waiting: Array<() => void> = [];

async function acquire(): Promise<void> {
  if (active < MAX_CONCURRENT) {
    active += 1;
    return;
  }
  await new Promise<void>((resolve) => {
    waiting.push(resolve);
  });
  active += 1;
}

function release(): void {
  active -= 1;
  waiting.shift()?.();
}

async function load(
  trackId: string,
  key: string,
  width: number,
  height: number,
  dpr: number,
  palette: WavePalette,
): Promise<RenderedWaveform | null> {
  const existing = inFlight.get(key);
  if (existing) return existing;

  const pending = (async () => {
    await acquire();
    try {
      const backend = await getBackend();
      const data = await backend.trackWaveform(trackId, waveformKindOf(palette, false));
      if (data.length === 0) return null;
      const rendered = await renderPreview(data, width, height, dpr, palette);
      if (rendered) cache.set(key, rendered);
      return rendered;
    } catch {
      // A track without analysis simply stays blank.
      return null;
    } finally {
      release();
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
  /**
   * The track's hot cues, drawn as lettered badges over the waveform.
   *
   * Painted onto the canvas after the cached bitmap rather than into it: the
   * bitmap is keyed by track and size and lives until evicted, and a cue
   * edited in the app would otherwise keep its old badge until then.
   */
  hotCues: readonly RowCue[];
  durationSec: number;
}

export const WaveformPreview = memo(function WaveformPreview({
  trackId,
  width,
  height,
  hotCues,
  durationSec,
}: WaveformPreviewProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  // View › Color › Waveform color: the row follows the deck's palette, and a
  // bitmap rendered in one palette is not the row in another.
  const { waveformColor: palette, hotCueColor } = usePreferences().view;

  useEffect(() => {
    let cancelled = false;
    const dpr = window.devicePixelRatio || 1;
    const key = `${palette}:${WaveformCache.key(trackId, width, dpr)}`;

    const paint = (entry: { bitmap: CanvasImageSource }) => {
      const canvas = ref.current;
      if (!canvas || cancelled) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(entry.bitmap, 0, 0, canvas.width, canvas.height);
      // CDJ: every badge the fallback green, whatever the cue was given.
      const badges = hotCueColor === "cdj" ? hotCues.map(([letter, at]) => [letter, at, null] as const) : hotCues;
      drawPreviewCues(ctx, badges, durationSec * 1000, canvas.width, dpr);
    };

    const cached = cache.get(key);
    if (cached) {
      paint(cached);
      return;
    }

    // Nothing is asked for until the row has settled. A row a flick goes past
    // is unmounted before this fires, and the request is never made.
    const timer = window.setTimeout(() => {
      void load(trackId, key, width, height, dpr, palette).then((rendered) => {
        if (rendered) paint(rendered);
      });
    }, SETTLE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [trackId, width, height, hotCues, durationSec, palette, hotCueColor]);

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
