/** Canvas drawing entry point — lazily chunked, kept off the cold-start path. */
export { bandStops, drawBands, drawPreview, drawPreviewCues, ramp, renderPreview, WaveformCache } from "./waveform";
export type { PreviewCue, RenderedWaveform, WaveBand } from "./waveform";
