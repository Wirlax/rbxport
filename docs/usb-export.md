# USB export preferences

**Delete music outside playlists** is off by default. When enabled, playlist
export and Sync Manager remove RBXport-exported audio outside all selected
playlists, including tracks previously exported individually. Original library
files and unrelated files on the USB are untouched. Cleanup occurs after the
new export is published. An empty selection is refused.

**Maximum CDJ compatibility** is also off by default. When enabled, export
converts incompatible mono/stereo audio, including FLAC and ALAC, into the chosen
format. It applies to playlist export, Sync Manager, and individual track export.

- **WAV** (default): 16-bit PCM, 44.1 kHz, stereo. Larger files, no lossy codec;
  higher-resolution sources are resampled and reduced to 16 bits.
- **MP3**: 320 kbps CBR, 44.1 kHz, stereo. Smaller files with lossy compression.

Compatible MP3 and integer PCM WAV/AIFF files at 44.1 or 48 kHz are copied as-is.
Choosing MP3 does not recompress already-compatible WAV or MP3 files. This targets
older **USB-capable** CDJs, not disc-only players. See the
[Pioneer CDJ-350 format specifications](https://www.pioneerdj.com/en/product/player/cdj-350/).
USB filesystem and player-specific library requirements still apply.

Conversion runs on export copies. It preserves source files and analysis timing,
updates the USB database/analysis paths, and reuses unchanged converted files on
later syncs. Changing the output format replaces the previous exported copy.
Conversion errors stop publication instead of publishing a partial audio file.
Surround audio is refused rather than silently dropping channels. WAV files over
the RIFF 4 GB limit are refused.

The app uses Symphonia decoding, Rubato resampling, PCM WAV writing, and bundled
LAME encoding. No FFmpeg executable or separate user installation is needed.

The small stereo FLAC test fixture is generated audio: a 440 Hz left channel and
880 Hz right channel at 96 kHz. Tests check stereo, duration, resampling alignment,
MP3 bitrate, incremental export, format changes, source preservation and failure
before publication. FFmpeg was used only to generate this fixture; tests do not
require it.
