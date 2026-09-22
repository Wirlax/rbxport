# USB analysis export repair, 2026-09-22

The affected USB B now exports the analysis that the CDJ-3000 actually opens.
The old export copied waveform files to an ID-based directory and left the
library's `?/filename` path in PPTH. The firmware calculates its analysis
folder from the final USB audio path, so it could browse and play a track
without finding those files. The library's share files also contain empty
cue lists; copying them did not export the cues held in `djmdCue`.

## Changes

- Calculate the analysis directory using firmware 3.20's UTF-16 path hash
  (`sub_12f3f28`), after filename sanitization, collision handling and conversion.
  The calculation reproduces all 61 independently authored reference paths.
  Resolve hash collisions by changing the audio filename and recalculating.
- Rewrite PPTH in DAT, EXT and 2EX to the final USB audio path. Keep waveform,
  grid, seek-table and unknown sections. Mask plaintext PSSI phrase data in
  the form rekordbox writes on export.
- Populate legacy and extended cue lists from the library, including hot
  cues, memory cues, loop endpoints, loop sizes, comments and colors. Source
  row order matters: descending djmdCue.rowid reproduces the reference lists.
  Populate OneLibrary cue rows too, while preserving device-only cue rows.
- Write the per-track hot-cue auto-load flag as `"ON"` in DeviceSQL string
  slot 7 as well as the OneLibrary boolean. Without the legacy string, saved
  markers appeared but pads recorded new points instead of recalling them
  with the deck set to REKORDBOX SETTING. Verification checks the two flags
  agree.
- Carry track/disc numbers, bit depth, play counts, analysis flags, hot-cue
  auto-load, creation dates and ISRC into exported metadata. Preserve these
  fields when carrying existing OneLibrary tracks through reconciliation.
- Verify player-discoverable analysis paths and embedded audio paths, parse
  all present companions and report waveform/grid/cue counts. Record expected
  companions and per-file hashes so a deleted EXT is detected and repairable
  without confusing it with a player cue edit.

## USB B read-back

A normal re-export wrote 183 analysis files for the existing 61 tracks and
one playlist. After a normal macOS unmount and remount, verification found:

| Asset | Count |
|---|---:|
| Audio tracks / playlist entries | 61 / 61 |
| Standard DAT analyses | 61 |
| Full-track waveform overviews | 61 |
| Detailed waveforms | 61 |
| Beat grids | 61 |
| Hot cues | 286 |
| Memory cues | 243 |
| Missing files or verification errors | 0 |

The 529 cue entries equal the live source-library count for this playlist.
All waveform sections, PQTZ grids, legacy/extended cue sections and PSSI
phrase sections match the rekordbox-authored USB A export byte for byte.
Remaining section differences are additional source PVDI sections in 61
2EX files and one additional empty PQT2 section; reference sections are not
missing. Whole-file byte equality is not claimed.

MYSETTING.DAT, MYSETTING2.DAT, DJMMYSETTING.DAT and djprofile.nxs match USB A
byte for byte. All five existing settings files, including DEVSETTING.DAT,
remain byte-identical to USB B's pre-repair copies. DEVSETTING.DAT retains
the previously documented difference from USB A at the unknown byte 0x78
and the dependent checksum.

## Player and regression validation

The CDJ-3000 firmware 3.20, running with synthesized peripherals, browsed all
61 playlist entries and loaded and played Freak and Run with both waveform
views and beat markers. Cue labels, colored hot-cue markers and the saved
loop appear in the player display. With HOT CUE AUTO LOAD set to REKORDBOX
SETTING, pad A recalled the saved Freak loop at approximately 134 seconds
and activated the saved loop. Memory-cue recall was also exercised. The test used
the final USB B track database with the new ON string, and matched the
reference database behavior. The final refresh reused all 183 analysis
companions and repaired the legacy database setting. This is firmware validation, not a test
on the user's physical CDJ-3000.

Regression tests cover reference binary cue records, comments/colors/loops,
metadata and cues in both databases, companion deletion and repair, removing
source cues on a subsequent sync, reference analysis paths, and an actual
hash collision between two different audio paths. The ANLZ, database,
OneLibrary and export suites pass (266 tests before the final auto-load
regression additions); the app's USB import tests pass.

Private local evidence and the pre-repair PIONEER backup are under
`verification/usbtest/2026-09-22-analysis/` (ignored by git). The hardware
reference records the hash and palette findings in its USB layout page.

The separate hardware-reference documentation gate still reports existing
em-dash violations and an empty docs directory outside this change. Code
lint and the focused regression checks pass.
