# The golden gate

Every change to this crate is judged against one thing: the playlist
`RBX-BPM-GRID-TEST` in the installed rekordbox library. 155 tracks, all 4/4,
123–178 BPM, each analysed by rekordbox and its grid checked or set by
hand. The rig is `examples/golden.rs`; it only reads the library.

## Running it

```
cargo run --release -p rbl-analysis --example golden -- cache
cargo run --release -p rbl-analysis --example golden -- eval
cargo run --release -p rbl-analysis --example golden -- eval <title substring>
cargo run --release -p rbl-analysis --example golden -- downbeat
cargo run --release -p rbl-analysis --example golden -- key
cargo run --release -p rbl-analysis --example golden -- bassroot
```

- `cache` decodes every track once and stores the mono PCM with what
  rekordbox recorded — BPM, key and the `PQTZ` beat grid — under
  `target/golden/` (3.6 GB, about two minutes). Delete a track's `.gold`
  file and run `cache` again after re-analysing it in rekordbox.
- `eval` scores every track in about five seconds. With a title substring
  it scores one track; add `RB_LITE_CANDIDATES=1` to also print the tempo
  candidates, the segments, the fit's passes, the half-beat scores and the
  first beats side by side with rekordbox's.
- `downbeat` checks the downbeat stage on rekordbox's own grids, so it is
  judged apart from our grid.
- `key` measures the key rules — what each fired on, fixed and broke — and
  searches the bass-rule knobs. `bassroot` measures the bass as evidence by
  itself, per band and window.

## What is scored

| metric | passes when |
|---|---|
| bpm | within 0.05 BPM of rekordbox's, no octave folding |
| downbeat | the offset between our downbeats and rekordbox's, taken modulo a bar, is within 25 ms |
| grid | at least 98 % of rekordbox's beats have one of ours within 25 ms carrying the same beat number |
| key | the same name rekordbox chose |

The grid metric is the strict one: a tempo right to 0.05 BPM can still
drift a beat off by the end of the track, and a tempo change has to be
placed at the right beat for the numbering after it to match.

## Results

| metric | start | now |
|---|---|---|
| bpm | 146 | **155 / 155** |
| downbeat | 27 | **151 / 155** |
| grid | 26 | **146 / 155** |
| key | 27 | **130 / 155** |

The target is 99 %, at most one miss per metric.

## The misses

**Eleven tracks analysed by an older rekordbox.** *See Me Now, technikore &
weaver – diya, Missing, Insane Stampede, Sky Fall, Concrete Jungle, Voltage,
Goddess, Drift Around Me, Angels, Force of Gravity.* Their rekordbox grids
sit exactly 1105 samples (25 ms) after the kick; the other 140 MP3s sit on
it. Chris confirmed the playlist was analysed at different times, some by
rekordbox 6 and some by 7. Our beats are on the kick, so these fail the
grid metric on the tolerance edge and two of them (Drift Around Me, Insane
Stampede, at −26 ms) fail the downbeat metric. Re-analysing them in
rekordbox 7 would settle it. Before that was known, the cause was chased
through the file and the decoder: it is not the Xing/Info frame, the LAME
tag's CRC, the encoder version, the ID3 version, or anything in
`djmdContent`; and rekordbox's own bundled `libmpg123`, driven from Python,
gives exactly our timeline (gapless off) or exactly 1105 samples earlier
(gapless on), never later.

**Two DJ edits, both gridded by hand.** `Go Back [136-174]`: 136 BPM until
bar 61, rising until bar 82.2, then 174; the hand grid holds 136 through the
rise and switches at bar 82.25. Ours switches at the same millisecond, but
our 136 line is 0.02 BPM off across an intro with no kicks (18 ms at the
first beat) and our beat 1 is one beat before the hand grid's. `Bring Me
Back to Life [138-150]`: the hand grid is anchored where the kicks state
the tempo and holds 138 until 105 s; the 150 beat is audible, quietly,
from 64 s and our switch goes there.

**BATTERY OPERATED.** Rekordbox's beats sit where every band of the
spectrum and the phrase structure say the off-beat is; the grid is half a
beat from ours.

**They Want Your Soul.** The comb says 133.003 BPM; the snap-and-refit
pulls it to 132.986, which drifts 22 ms by the end; 86 % of beats match.
The refit stays because it is right more often than the comb alone (with
the comb alone the grid metric drops from 146 to 143).

**Key.** See [key.md](key.md): 11 mode misses, 8 fifths, 6 others.

## Rekordbox facts learned on the way

- Rekordbox's grid is on the untrimmed MP3 timeline. A track whose first
  kick is at sample 0 of the master has its first beat at ~24 ms — the LAME
  encoder delay plus decoder delay, 1105 samples. Symphonia with
  `enable_gapless: false` matches it.
- Rekordbox 7 bundles `libmpg123.0.dylib` in `rekordbox.app/Contents/MacOS`.
  Its output here is 32-bit float.
- Rekordbox's grids are constant-tempo to within 1–3 ms over a whole
  track. A tempo change is a second constant grid with the 1–4 count
  carrying on and a new phase.
- Rekordbox starts the grid at the first grid position after time zero,
  extended back from the music, even over silence or a beatless intro.
- Ten of the 155 tracks have beat 1 one to three beats after the first
  beat.
- `.EXT` files can carry an empty `PQT2`; the `.DAT`'s `PQTZ` is the grid.
