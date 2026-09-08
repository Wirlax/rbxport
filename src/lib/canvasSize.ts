/**
 * How big a canvas's backing store should be for the space it occupies.
 *
 * A canvas stretched by CSS from a fixed backing store is blurry on a wide
 * window and wasteful on a narrow one, so the store follows the element. Two
 * things stop that being expensive:
 *
 * - the width is quantised, so dragging the window edge reallocates the buffer
 *   a handful of times rather than once per pixel;
 * - the device pixel ratio is capped, because past 2× the extra samples are
 *   not visible in a 14-pixel-high waveform and the buffer grows with the
 *   square of the ratio.
 */

/** Reallocate in steps this wide rather than on every pixel of a drag. */
export const STEP = 16;

/** Beyond this the extra resolution is not visible in a waveform strip. */
export const MAX_RATIO = 2;

/** Guards against a runaway layout asking for a buffer megabytes wide. */
export const MAX_WIDTH = 4096;

export interface BackingSize {
  width: number;
  height: number;
}

export function backingSize(
  cssWidth: number,
  cssHeight: number,
  ratio: number = typeof window === "undefined" ? 1 : window.devicePixelRatio,
): BackingSize {
  // `Math.max(NaN, 1)` is NaN, and a canvas given a NaN width silently keeps
  // its old buffer — so anything not a real number becomes the floor here.
  const scale = Math.min(Math.max(finite(ratio, 1), 1), MAX_RATIO);
  const stepped = Math.ceil(Math.max(finite(cssWidth, 1), 1) / STEP) * STEP;
  return {
    width: Math.min(MAX_WIDTH, Math.max(STEP, Math.round(stepped * scale))),
    height: Math.max(1, Math.round(Math.max(finite(cssHeight, 1), 1) * scale)),
  };
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}
