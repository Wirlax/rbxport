/** Canvas drawing entry point — lazily chunked, kept off the cold-start path. */
export {
  bandStops, drawBands, drawColumns, drawPreview, drawPreviewCues, drawWave, ramp, renderPreview,
  strideOf, waveformKindOf, WaveformCache,
} from "./waveform";
export type { HalfWaveform, PreviewCue, RenderedWaveform, WaveBand, WavePalette } from "./waveform";
