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

/** Column colours, from the measured palette. */
const LOW = [0x00, 0x55, 0xe1] as const;
const HIGH = [0xf5, 0xea, 0xd6] as const;

function mix(a: readonly number[], b: readonly number[], t: number): string {
  const c = (i: number) => Math.round((a[i] ?? 0) + ((b[i] ?? 0) - (a[i] ?? 0)) * t);
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
): void {
  ctx.clearRect(0, 0, width, height);
  if (data.length === 0) return;

  const step = data.length / width;
  for (let x = 0; x < width; x++) {
    // Take the loudest column in this pixel's span so quiet gaps do not
    // swallow transients when many columns share a pixel.
    let peak = 0;
    let whiteness = 0;
    const from = Math.floor(x * step);
    const to = Math.max(from + 1, Math.floor((x + 1) * step));
    for (let i = from; i < to && i < data.length; i++) {
      const byte = data[i] ?? 0;
      const h = byte & HEIGHT_MASK;
      if (h > peak) {
        peak = h;
        whiteness = byte >> WHITENESS_SHIFT;
      }
    }
    if (peak === 0) continue;
    const columnHeight = Math.max(1, Math.round((peak / HEIGHT_MASK) * height));
    ctx.fillStyle = mix(LOW, HIGH, whiteness / 7);
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
