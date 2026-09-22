# CDJ-3000 empty USB playlists: track records, 2026-09-22

USB B contained 61 tracks and 61 entries in `NP3-TEST-MP3`. The structural
verifier passed, but the physical CDJ-3000 displayed an empty playlist. A
local copy reproduced the failure in the CDJ-3000 firmware emulator. This
is the previously unresolved player failure in [usb-sync.md](usb-sync.md).

The writer left required track-record words at zero. The semantic reader
uses fixed offsets and ignored these words, so reading our own output back
could not detect the problem. Playlist membership itself was present.

| Track-relative offset | Old writer | Corrected writer |
| --- | --- | --- |
| `0x00` | 0 | `0x0024`, track subtype using 16-bit string offsets |
| `0x56` | 0 | `0x0029`, captured record trailer word |
| `0x5a` | 0 | Audio format: MP3=1, M4A/AAC=4, FLAC=5, WAV=11, AIFF=12 |
| `0x5c` | 0 | 3, captured record trailer word |

The format code comes from the **exported** filename, so a FLAC converted
for compatibility writes the WAV or MP3 code. The format mapping also
matches the library importer's observed codes. The exact roles of the
trailer words at `0x56` and `0x5c` remain uncharacterized; their values are
captured from rekordbox's reference, not inferred as counts or flags.

Isolated firmware tests on local copies:

- Original export: empty track/artist lists and a named, empty playlist.
- Only `0x5c = 3`: still empty.
- Only subtype `0x24`: 61 playlist entries, but broken titles and an
  unsupported-file-format error on load.
- Subtype plus the three trailer words above: 61 entries, readable titles,
  track load and playback. No changes to page headers, row numbering,
  bitmasks, track IDs, playlist IDs, paths or membership were needed.
- A fresh 61-track export from the corrected Rust writer: the firmware
  displays `[0061]`; both `Freak (feat. Aya Anne) (feat. Aya Anne)` and
  `Run (Original Mix)` load and play, with the remaining time advancing.

`verification.rs` now checks raw record subtype, nonzero trailer words,
and the audio format for recognized extensions before publication and
on explicit verification. The snapshot reader remains permissive so normal
reconciliation can read and repair an older malformed export.

Validation: 81 PDB/export tests pass, including direct byte assertions
against the player-accepted record layout, format conversion checks, and
corrupt-record rejection followed by successful incremental repair.
Clippy for both crates and the debug desktop build pass.

Local captures, OCR, scripts, and the original USB database backup are in
`verification/usbtest/2026-09-22-track-records/`. USB B's repair changes only
four low bytes per track (244 bytes total), retaining the existing database
layout, audio, analysis, playlists and settings. See `repair.json` for the
before/after hashes. Verification passed again after unmount/remount,
the repaired PDB hash was unchanged, and USB B was safely ejected.

The playback validation is firmware-emulator validation, not a physical
CDJ retest. This fixes the empty-playlist defect, not full USB parity:
detail waveforms remain absent in these captures. The firmware logs an
analysis-directory mismatch, and the earlier audit documents additional
analysis differences. Those require separate investigation.
