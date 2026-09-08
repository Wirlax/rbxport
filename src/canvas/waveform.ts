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
 * How tall one band's value can be.
 *
 * Seven bits. Measured across 80 tracks of the reference library: `PWV7`
 * reaches the full 127 and `PWV6` reaches 98, so both are on the same scale.
 * An earlier reading of 63 came from a single track and drew every overview at
 * double height.
 */
export const BAND_FULL_SCALE = 127;

/**
 * How tall the three bands stacked can be.
 *
 * Not three times the band scale: the bands do not peak together. Over 96,000
 * columns of `PWV6` the largest stack was 147, so anything past this is a
 * peak worth clipping rather than headroom worth reserving — reserving it
 * would draw every row preview at a third of its height.
 */
export const STACK_FULL_SCALE = 150;

/**
 * Draws a three-band waveform: `PWV6` or `PWV7`, three bytes a column.
 *
 * This is what rekordbox 7 actually shows, and what a CDJ-3000 shows. The
 * bytes are the energy in the low, mid and high thirds of the spectrum, and
 * each is drawn from the centre line in its own colour: blue underneath, amber
 * over it, near-white on top. Highs carry the least energy — a mean of 3
 * against 30 for the other two on a real track — so drawing them last is what
 * puts the bright core in the middle rather than burying it.
 */
export function drawBands(
  ctx: CanvasRenderingContext2D,
  data: Uint8Array,
  width: number,
  height: number,
  band: WaveBand = "overview",
  /**
   * Half height, growing from the bottom, with the bands stacked rather than
   * centred and overlaid.
   *
   * This is the row preview in the track list. Stacking is what gives the blue
   * its flat top with the amber riding above it — overlaying from a baseline
   * would hide the amber entirely whenever the lows are louder, which is most
   * of the time.
   */
  half = false,
  /**
   * Rows left clear at the top and bottom, in device pixels.
   *
   * rekordbox's waveform does not reach the edges of its band: the strip above
   * it carries the bar count and the heads of the cue markers. Measured at 8pt
   * above and 2pt below.
   */
  inset: { top: number; bottom: number } = { top: 0, bottom: 0 },
): void {
  ctx.clearRect(0, 0, width, height);
  const columns = Math.floor(data.length / 3);
  if (columns === 0 || width <= 0 || height <= 0) return;

  const stops = bandStops(band);
  // The band the waveform actually draws into. Clamped so a large inset on a
  // short strip leaves something rather than inverting it.
  const top = Math.max(0, Math.min(inset.top, height / 2 - 1));
  const bottom = Math.max(0, Math.min(inset.bottom, height / 2 - 1));
  const usable = Math.max(1, height - top - bottom);
  const floor = height - bottom;
  const centre = top + usable / 2;
  const step = columns / width;

  for (let x = 0; x < width; x++) {
    // The loudest column in this pixel's span, per band, so a transient is not
    // swallowed when many columns share a pixel.
    let low = 0;
    let mid = 0;
    let high = 0;
    const first = Math.floor(x * step);
    const last = Math.max(first + 1, Math.floor((x + 1) * step));
    for (let i = first; i < last && i < columns; i++) {
      const at = i * 3;
      low = Math.max(low, data[at] ?? 0);
      mid = Math.max(mid, data[at + 1] ?? 0);
      high = Math.max(high, data[at + 2] ?? 0);
    }
    const bands = [
      [low, stops[0]],
      [mid, stops[1]],
      [high, stops[2]],
    ] as const;

    if (half) {
      // Stacked from the bottom: blue, then amber on it, then near-white.
      let base = floor;
      for (const [value, colour] of bands) {
        if (value === 0) continue;
        const tall = (Math.min(value, STACK_FULL_SCALE) / STACK_FULL_SCALE) * usable;
        ctx.fillStyle = `rgb(${colour?.[0] ?? 0},${colour?.[1] ?? 0},${colour?.[2] ?? 0})`;
        ctx.fillRect(x, Math.max(top, base - tall), 1, Math.min(tall, base - top));
        base -= tall;
        if (base <= top) break;
      }
      continue;
    }

    // Centred: low first so the blue is the outer envelope, high last so the
    // bright core sits on top of both.
    for (const [value, colour] of bands) {
      if (value === 0) continue;
      const reach = Math.max(0.5, (Math.min(value, BAND_FULL_SCALE) / BAND_FULL_SCALE) * (usable / 2));
      ctx.fillStyle = `rgb(${colour?.[0] ?? 0},${colour?.[1] ?? 0},${colour?.[2] ?? 0})`;
      ctx.fillRect(x, centre - reach, 1, reach * 2);
    }
  }
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
  // The row preview: half height from the baseline, bands stacked.
  drawBands(ctx, data, w, h, "overview", true);

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
