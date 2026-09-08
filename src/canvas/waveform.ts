/**
 * Waveform drawing.
 *
 * rekordbox's `PWAV` preview packs each column into one byte: the low five bits
 * are the height and the top three a "whiteness" that shades the column. Drawing
 * that faithfully is what makes a preview recognisable rather than a grey blob.
 */

/** A rendered preview, cached as a bitmap so scrolling is a `drawImage`. */
export interface RenderedWaveform {
  bitmap: ImageBitmap | HTMLCanvasElement;
  width: number;
  height: number;
}

const HEIGHT_MASK = 0x1f;
const WHITENESS_SHIFT = 5;

/**
 * Which waveform is being drawn.
 *
 * Only the middle band differs: the overview strip uses a brighter amber than
 * the detail does. Both are in the tokens, measured off the capture.
 */
export type WaveBand = "overview" | "detail";

/**
 * The three bands, from `src/styles/tokens.css`.
 *
 * rekordbox colours a column by its frequency content: bass blue, mids amber,
 * highs near-white. `PWAV` gives three bits of "whiteness" per column, which
 * is that spectrum in miniature, so the ramp runs through all three rather
 * than straight from blue to white — a two-stop ramp turns every mid-heavy
 * track grey-blue and loses the thing that makes a waveform readable.
 */
const LOW = [0x00, 0x55, 0xe1] as const; // --c-wave-low  #0055E1
const MID_DETAIL = [0xb2, 0x65, 0x05] as const; // --c-wave-mid  #B26505
const MID_OVERVIEW = [0xff, 0x8c, 0x00] as const; // --c-wave-mid-ovw #FF8C00
const HIGH = [0xf5, 0xea, 0xd6] as const; // --c-wave-high #F5EAD6

export function bandStops(band: WaveBand): readonly (readonly number[])[] {
  return [LOW, band === "overview" ? MID_OVERVIEW : MID_DETAIL, HIGH];
}

/** Colour at `t` (0..1) along a ramp through every stop in turn. */
export function ramp(stops: readonly (readonly number[])[], t: number): string {
  const last = stops.length - 1;
  if (last < 1) {
    const only = stops[0] ?? LOW;
    return `rgb(${only[0] ?? 0},${only[1] ?? 0},${only[2] ?? 0})`;
  }
  const clamped = Math.min(Math.max(Number.isFinite(t) ? t : 0, 0), 1);
  const scaled = clamped * last;
  const i = Math.min(Math.floor(scaled), last - 1);
  const a = stops[i] ?? LOW;
  const b = stops[i + 1] ?? HIGH;
  const f = scaled - i;
  const c = (n: number) => Math.round((a[n] ?? 0) + ((b[n] ?? 0) - (a[n] ?? 0)) * f);
  return `rgb(${c(0)},${c(1)},${c(2)})`;
}

/**
 * Draws a `PWAV` preview into a context.
 *
 * `data` is one byte per column. The canvas is scaled to fit however many
 * columns there are, so a 400-column preview fills a 120px cell.
 */
export function drawPreview(
  ctx: CanvasRenderingContext2D,
  data: Uint8Array,
  width: number,
  height: number,
  options?: {
    /**
     * The slice of the track to draw, as fractions of its length. Defaults to
     * all of it; the detail waveform passes a window around the playhead,
     * which is what makes it a *detail* rather than a second copy.
     */
    window?: { from: number; to: number };
    /** Which palette. Defaults to the overview's brighter amber. */
    band?: WaveBand;
  },
): void {
  const window = options?.window;
  const stops = bandStops(options?.band ?? "overview");
  ctx.clearRect(0, 0, width, height);
  if (data.length === 0) return;

  // Clamped and ordered, so a playhead at either end still draws something.
  const from = Math.max(0, Math.min(window?.from ?? 0, 1));
  const to = Math.max(from + 1e-6, Math.min(window?.to ?? 1, 1));
  const first = Math.floor(from * data.length);
  const last = Math.max(first + 1, Math.ceil(to * data.length));
  const span = last - first;

  const step = span / width;
  for (let x = 0; x < width; x++) {
    // Take the loudest column in this pixel's span so quiet gaps do not
    // swallow transients when many columns share a pixel.
    let peak = 0;
    let whiteness = 0;
    const columnFrom = first + Math.floor(x * step);
    const columnTo = Math.max(columnFrom + 1, first + Math.floor((x + 1) * step));
    for (let i = columnFrom; i < columnTo && i < data.length; i++) {
      const byte = data[i] ?? 0;
      const h = byte & HEIGHT_MASK;
      if (h > peak) {
        peak = h;
        whiteness = byte >> WHITENESS_SHIFT;
      }
    }
    if (peak === 0) continue;
    const columnHeight = Math.max(1, Math.round((peak / HEIGHT_MASK) * height));
    ctx.fillStyle = ramp(stops, whiteness / 7);
    ctx.fillRect(x, height - columnHeight, 1, columnHeight);
  }
}

/** Renders a preview to an offscreen bitmap at device resolution. */
export async function renderPreview(
  data: Uint8Array,
  width: number,
  height: number,
  dpr: number,
): Promise<RenderedWaveform | null> {
  const w = Math.max(1, Math.round(width * dpr));
  const h = Math.max(1, Math.round(height * dpr));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  drawPreview(ctx, data, w, h);

  // An ImageBitmap blits faster than a canvas element; fall back where the
  // browser lacks it rather than failing to draw at all.
  if (typeof createImageBitmap === "function") {
    try {
      return { bitmap: await createImageBitmap(canvas), width: w, height: h };
    } catch {
      // fall through
    }
  }
  return { bitmap: canvas, width: w, height: h };
}

/** Bounded cache of rendered previews, keyed by track and size. */
export class WaveformCache {
  #entries = new Map<string, RenderedWaveform>();
  #max: number;

  constructor(max = 500) {
    this.#max = max;
  }

  static key(trackId: string, width: number, dpr: number): string {
    return `${trackId}:${Math.round(width)}:${dpr}`;
  }

  get(key: string): RenderedWaveform | undefined {
    const hit = this.#entries.get(key);
    if (hit) {
      this.#entries.delete(key);
      this.#entries.set(key, hit);
    }
    return hit;
  }

  set(key: string, value: RenderedWaveform): void {
    this.#entries.delete(key);
    this.#entries.set(key, value);
    while (this.#entries.size > this.#max) {
      const oldest = this.#entries.keys().next();
      if (oldest.done) break;
      const evicted = this.#entries.get(oldest.value);
      // Release the GPU-side copy explicitly; the GC will not do it promptly.
      if (evicted && "close" in evicted.bitmap) evicted.bitmap.close();
      this.#entries.delete(oldest.value);
    }
  }

  get size(): number {
    return this.#entries.size;
  }

  clear(): void {
    for (const entry of this.#entries.values()) {
      if ("close" in entry.bitmap) entry.bitmap.close();
    }
    this.#entries.clear();
  }
}
