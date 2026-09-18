# The multi-tempo test

The golden gate scores the analysis against rekordbox's stamps in a cache.
This test puts it in front of rekordbox: the playlist `RBX-BPM-MULTIBPM-TEST`
holds nine DJ edits with tempo changes, each gridded by hand in rekordbox.
The rig `examples/multibpm.rs` copies their files, imports the copies into
`RBX-BPM-MULTIBPM-RESULTS`, analyses them, and registers the result in the
library the way rekordbox registers its own, so a copy can be loaded on a
deck beside its original. `report` then scores the copies with the golden
gate's metrics and prints both grids as tempo runs. The Claude skill
`.claude/skills/multibpm-test` runs it for all songs or one.

```
cargo run --release -p rbl-analysis --example multibpm -- run [filter]
cargo run --release -p rbl-analysis --example multibpm -- stage|reset|import|analyse|report|check [filter]
```

rekordbox and its agent must be quit: the writer refuses otherwise, and takes
a backup of `master.db` (under `target/multibpm/backups`) before the first
write of a run.

## What is written, and the evidence for it

Only rows the rig imported itself, recognised by their file living under
`target/multibpm/tracks`, are deleted, re-imported or registered.

**The analysis files.** A `.DAT` and `.EXT` under
`share/PIONEER/USBANLZ/<uuid[0..3]>/<uuid[3..]>/ANLZ0000.DAT`, section for
section as rekordbox writes them. Every section header was checked against
400 reference files and is constant [OBS]:

| file | sections |
|---|---|
| `.DAT` | `PPTH`, `PVBR` (zero table, as on 65 of 400 real files), `PQTZ`, `PWAV` (400 columns), `PWV2` (100), `PCOB` ×2 |
| `.EXT` | `PPTH`, `PWV3` (150/s), `PCOB` ×2, `PCO2` ×2, `PQT2` (empty, as on 36 of 400), `PWV5` (150/s), `PWV4` (1,200) |

`PPTH` is `?/<file name>` on all 300 real files sampled, not the full path.
Not authored: `PSSI` (phrases), `PVDI` (vocals) and the `.2EX` (`PWV6`/`PWV7`
three-band waveforms), so rekordbox draws the copies with its RGB or BLUE
waveform and no phrase strip. The waveform packers are in `waveform.rs`;
the byte ranges (`PWV2` ≤ 15, `PWV4` height to 255 and channels to 127) are
the reference files' [OBS]. Which band drives which colour channel is our
mapping, not rekordbox's [UNKNOWN].

**The row.** `Writer::register_analysis` in `rbl-db` sets `BPM`, `KeyID`,
`AnalysisDataPath` and `Analysed`, in one transaction with one USN, the
values read off the reference library's 38,681 rows rather than a diff
recording:

- `AnalysisDataPath` is derived from the row's `UUID` on 38,674 rows.
- `Analysed = 105` on 37,663 rows, and on every row whose files are present;
  1 and 17 appear only on rows whose files are missing.
- `KeyID` is the `djmdKey` row rekordbox's own analyses use for that name —
  some names have two rows (`Cm` is 3793949082 on 2,418 tracks and
  2794270948 on 13).
- `AnalysisUpdated` (0–10) is left alone; its meaning is [UNKNOWN].

The first time rekordbox opens the results playlist is therefore the
recording this was missing: `check` compares the files and columns with what
`analyse` wrote (kept in `target/multibpm/registered.tsv`) and reports
anything rekordbox changed.

## Results

First run, 2026-09-17. BPM 9 / 9, key 9 / 9, downbeat 7 / 9, grid 1 / 9.
The bpm and key are right on every edit; what this playlist tests is
where the tempo change is placed, and that is where the misses were.

Second measurement, later the same day, read-only (`golden cache
RBX-BPM-MULTIBPM-TEST` into `RB_LITE_GOLDEN=target/multibpm-gold`, then
`golden score`): BPM 9 / 9, key 9 / 9, downbeat 7 / 9, grid 4 / 9. The
change went in with the cut placed where the kick states the new tempo
at full level ([beat.md](beat.md), step 6), runs of fewer than three
settled windows absorbed, the count carried across joins, a new segment
starting with the beat it was cut on, and every steady tempo a whole
number. Seven of the ten changes are now within 3 ms of the hand grid.

| track | grid | what differs |
|---|---|---|
| Go Back [136-174] | 89 % | the cut is at 143.340 s against 143.342 s and numbered as the hand grid numbers; the hand grid nudges its 136 by half a beat somewhere in the rise (bars 61–82) and ours holds it straight |
| Bring Me Back to Life [138-150] | 100 % | 105.156 s against 105.158 s |
| Castles In The Sky (TRIODE Festival Edit) | 100 % | the one constant-tempo track |
| Around the World [134-150] | 0 % | beat 1 one beat off through the whole track (our beat 1 on rekordbox's beat 2), a downbeat-stage miss on the 134 section; the change itself is at 171.813 s against 171.815 s |
| It Feels So Good [150-134] | 100 % | 125.899 s against 125.900 s |
| Cannonball [136-150-136] | 16 % | the return to 136 at 217.186 s against 217.187 s; but the hand grid cuts to 128 at 56.489 s and to 145 at 86.032 s at the impacts that end each section, with no kick at the new tempo for twenty seconds after either, and ours cuts at 67.3 s (where the onsets change sides) and at 109.95 s (where the 145 kick arrives), so the count is off from 56 s to 217 s |
| Sao Paulo | 62 % | the 160 stretch is at 133.523 s against 133.524 s; the hand grid returns to 128 at 157.524 s at the impact that ends the 160 section, with the 128 kick arriving at 196 s, and ours cuts there |
| Castles In The Sky (EDCLV23 Closer) [138-160] | 100 % | the spurious 162.6 stretch is gone (one window at 1.19 had been assigned to the 160 cluster at 1.16) and the change is at 233.260 s exactly |
| BATTERY OPERATED | 0 % | half a beat off, as on the golden gate (the ambiguous track) |

The three remaining cut misses are one kind: the hand grid switches at
the impact that ends the old section, and the new tempo's kick comes
much later. Onsets alone cannot tell that impact from the one that
starts a breakdown in the middle of a section (Bring Me Back to Life at
60 s, which the hand grid holds through), so the rule stays "where the
kick states the tempo". The registered copies in the library are from
the first run; `run` (rekordbox quit) refreshes them.
BATTERY OPERATED was not ambiguous: its hand grid is two 130 lines 208 ms
apart, joined by a slowdown with no kick ([golden-gate.md](golden-gate.md)).
The gap stage built on it cuts where the hand grid does and grids the
slowdown; the copy in the results playlist was registered before that and
shows the old grid until `analyse` is run again.

### What rekordbox did with the copies

The first import showed every copy with the missing-file mark and none
would load. Our importer had left empty the columns rekordbox fills on its
own imports — `FileType`, `DeviceID`, `MasterDBID`, `BitDepth`,
`StockDate`, `DateCreated` and a handful of constants (all 645 rows
rekordbox 7 imported on this machine carry them [OBS]) — and stored a path
with `../..` in it. `Writer::import_file` now writes all of them and a
lexically clean path; the copies then load.

Loading a copy on a deck, rekordbox 7.2.11 (recorded 2026-09-17):

- kept the `.DAT`, so the grid on the deck is ours, and kept `BPM`, `KeyID`
  and `Analysed`;
- rewrote the `.EXT` in place — same sections, same sizes, its own
  waveform bytes, `PQT2` still empty, no `PSSI`;
- added a `.2EX` (`PWV6`, `PWV7`, `PWVC`, no `PVDI`) and a `.3EX`;
- set `AnalysisUpdated` and `TrackInfoUpdated` from NULL to 1.

Relaunching rekordbox alone touches nothing. `check` reports all of this
per copy and says whether the grid survived.
