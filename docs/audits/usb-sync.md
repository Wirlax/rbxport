# USB synchronization audit — 2026-09-21

Scope: legacy Device Library (`export.pdb`), Device Library Plus / OneLibrary
(`exportLibrary.db`), their sync records, exported assets, and return imports
into the master library.

## Implemented protections

- Creating a missing database sibling converts the existing library instead
  of replacing its contents with an empty library. Unknown OneLibrary schema
  versions fail before publication.
- Export identities include the originating master database and content IDs.
  Manifests carry a semantic baseline; foreign-library manifests and detected
  conflicting device edits fail instead of silently overwriting those edits.
- Device-only playlists, folders, referenced tracks, play histories, My Tags,
  and existing OneLibrary cue rows survive reconciliation. Legacy history uses
  the actual played-history tables (11 and 12).
- Playlist IDs and hierarchy survive reorder. Removing the final selected
  playlist is supported. Hidden `.PIONEER` roots remain hidden; competing roots
  are rejected.
- Missing selected source files abort before publication. Unicode names are
  truncated at character boundaries; colliding destination names get distinct
  paths. Audio and artwork reuse checks compare content, not merely size.
- Changed analysis companions are published together and obsolete companions
  are removed. Device cue/grid changes and edits made during staging are
  checked before commit.
- Staged verification checks agreement between both library formats, playlist
  references/hierarchy, required audio and analysis, and history references.
  Publication uses the recoverable file journal, checks mount identity and
  space, and serializes cooperating readers/writers.
- Return imports support legacy histories and explicit master identities.
  Manifest fallback validates the source path and rejects a known foreign
  database ID. Tracks without analysis can still be matched for history.

## Validation

The 16 export safety tests cover missing sources, Unicode/path collisions,
foreign identities, conflicting metadata edits, retained device playlists and
history, sync-to-empty, stable folder IDs, invalid exports, either database
being absent, hidden roots, Plus-only sync records, same-size corruption,
unsupported schemas, stale analysis companions, late edits, and retained tags
and SQL cue rows. The three Tauri USB import tests also pass.

Physical testing uses the two external removable FAT32 sticks named USB A
and USB B. Original contents were backed up under
`target/usb-sync-backup-20260921/`. Both sticks were formatted and explicitly
unmounted/remounted. Spotlight temporarily prevented the first unmount; a
normal retry succeeded. The reset command now retries this condition and
verifies the remounted device identity. The GUI helper now verifies that Sync
Manager closes instead of treating an attempted click as success.

The clean RBX export wrote 61 tracks, one playlist and 183 analysis files
(796 MB) in 124.3 seconds. Verification after unmount/remount found 61 audio
files, 61 DAT analyses, 61 playlist entries, and no errors. A second export
passed in 39.9 seconds and wrote zero analysis files.

The full comparison **failed**: 309 identical files, three database files
with matching compared content, three byte differences, 187 only in A and
185 only in B. Both sync-record differences were exclusively timestamps.
The differing Husko MP3 was byte-identical to the source on B; A changed its
ID3v2 tags and removed its 128-byte ID3v1 trailer. The MPEG audio bytes were
identical. Analysis-directory naming accounts for 183 unmatched files on
each stick. Other differences include credentials, backup information,
DEVSETTING.DAT, WAL/SHM sidecars and the RBX manifest. The database comparator
checks legacy rows, extended tags, and OneLibrary settings/counts; it is not
an exhaustive equality proof for every OneLibrary column.

Logs: `target/usb-sync-clean-compare.log`,
`target/usb-sync-remount-verify.log`, `target/usb-sync-incremental.log`.
The broader Rust suites passed 131 tests; the three USB import tests passed,
including a regression assertion for tracks without analysis.

### Full parity and atemu follow-up

**Verdict: full export parity FAIL; requested settings PASS; CDJ-3000
emulator compatibility FAIL.** The earlier count-based result is insufficient
to qualify a USB export for player use.

The complete settings change set was applied in real rekordbox to USB A and
compared with RBX's USB B. Device name `PARITY`, RGB, LEFT, Full Waveform,
Alphanumeric, active BPM category, inactive BPM sort, BPM sub-column and
`Pink` → `Vocal` agree. The complete OneLibrary category, sort, color, menu and
playlist tables agree. MYSETTING.DAT, MYSETTING2.DAT and DJMMYSETTING.DAT are
byte-identical. DEVSETTING.DAT differs at unknown byte 0x78 (A=2, B=0), plus
its CRC at 0x88–0x89. This is not byte-for-byte settings-file parity.

The post-settings file comparison reports 309 identical files, three files
matching the old limited database comparison, four byte differences, 187
files only in A and 184 only in B. Raw comparison artifacts are under
`verification/usbtest/2026-09-21-full/`.

Full OneLibrary column inspection found additional differences: trackNo
differs on 59 tracks; fileType, bitDepth, analysedBits, hot-cue auto-load,
creation date and other content fields are NULL on B where A populates them;
play counts are zero on B. Artwork references select `aN.jpg` rather than
`bN.jpg`; all 99 My Tag sequence values differ. Some other differences are
default-vs-NULL representations or known path/identity derivations, and are
reported without assuming they are harmless. File sizes differ on 59 rows;
RBX records actual source-file size while rekordbox can carry library values.
The reusable read-only diagnostic is `scripts/usbtest/compare-columns.py`.

Matching analysis files by track identity, rather than directory name, found
all 183 files differ. Differing sections include PPTH paths, PCOB/PCO2 cue
sections, PSSI, PVDI, and one PQT2. For the first track, A's PPTH is the full
USB `/Contents/...` path while B retains `?/filename.mp3`. This requires
semantic analysis/cue validation; directory naming alone does not explain
the differences. MPEG audio matches on all tracks; the one differing MP3
has tag changes only.

atemu ran a dedicated CDJ-3000 instance on a private NAT network, with copied
exports and a power cycle between media selections. Real rekordbox's A
export shows 61 songs, all 61 playlist entries, waveforms and artwork; the
first track loads and plays (screenshot `atemu-a-playing.png`). The original
RBX B export shows zero songs and one named but empty playlist (screenshots
`atemu-usb-b-boot.png`, `atemu-usb-b-playlists.png`). Therefore the emulator
provides a positive control and a reproducible RBX compatibility failure.
Two isolated PDB header/row-flag experiments did not fix it and were discarded;
no speculative binary changes were applied to production code or left on USB.

Native rekordbox recognized USB B and displayed its waveform settings, but
immediately began automatic sync from its sync record. It was stopped; that
attempt is not evidence of unchanged-device read-back. USB B was restored
from the saved RBX export, both sticks were unmounted/remounted, and the
structural verifier again passed for all 61 tracks. That structural pass
does not override the failed emulator result. Rekordbox, atemu and the dialog
watcher were closed at the end.

## Remaining validation limits

- A journal provides recovery, not an atomic filesystem-wide switch visible
  to unrelated applications. Recover an interrupted publication in RBX before
  opening the stick in another application or player.
- Physical power loss, controller caches, damaged media, and playback on real
  DJ hardware have not been validated. The atemu result above is emulator
  validation, not a physical CDJ test.
- Conflicting device edits are detected and preserved by refusing export;
  this is not automatic bidirectional merging of every database field.
- Existing SQL cue rows are retained; new cue export still depends on ANLZ.
  Unmodeled OneLibrary ancillary tables are not covered by a preservation
  guarantee.
- Known parity differences include analysis-directory naming, the cloud
  credential file, and My Tag master-ID derivation. A failed full comparison
  must not be reported as a parity pass merely because these are known.
