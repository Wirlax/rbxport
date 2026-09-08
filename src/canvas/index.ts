/** Canvas drawing entry point — lazily chunked, kept off the cold-start path. */
export { bandStops, drawBands, drawPreview, ramp, renderPreview, WaveformCache } from "./waveform";
export type { RenderedWaveform, WaveBand } from "./waveform";
