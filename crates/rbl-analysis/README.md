# rbl-analysis

Tempo, beat grid, downbeat, key and waveform from decoded audio. Everything
is offline and deterministic: the same samples always give the same result,
which is what lets the output be compared against rekordbox's own stamps as
a regression gate.

The input is mono `f32` at the file's sample rate, from `rbl-audio`. The
output is an `Analysis`: a `TempoResult` (BPM, tempo segments, every beat
with its number in the bar), an optional `MusicalKey`, a three-band
`Waveform`, and the track's peak and RMS.

```
samples ──► onset envelope ──► candidates ──► fit ──► segments ──► beats
                                                                     │
samples ──► half-beat profiles ──► novelty peaks ──► bar position ───┴──► moved and renumbered beats
samples ──► chromagram ──► profile match ──► key
samples ──► waveform
```

Phrase (`PSSI`) and vocal (`PVDI`) detection are declared as traits with
unimplemented stubs, so the ANLZ writer can omit those tags rather than write
invented structure.

## Onset envelope (`onset.rs`)

The tempo stages never look at the audio; they look at an *onset envelope*,
one number per 256-sample hop (172 values per second at 44.1 kHz) saying how
much a percussive onset happened there.

It is spectral flux: a 1024-sample Hann-windowed FFT every hop, and for each
frame the sum over bins of the *increase* in magnitude since the previous
frame. Only increases count, so steady tones and decays contribute nothing
and a kick, which raises many bins at once, stands out. A local mean over
±16 samples is subtracted and the result clipped at zero, which removes slow
loudness drift so a quiet intro and a loud drop weigh the same; then the
whole envelope is scaled to a peak of 1.

Each value is timestamped at the centre of its frame (`origin_secs`), not
its start. An onset raises the spectrum most as the window's peak passes
over it, so the flux peaks when the centre reaches the onset. Timestamping
at the start put every beat 12 ms early, half the tolerance the grid is
judged by.

`onset_envelope_band` restricts the sum to a frequency band and lets the band
carry its own frame size. The full band is what tempo uses. A low band was
measured against it for choosing the beat and lost (see Downbeat).

## Tempo and grid (`tempo.rs`)

### 1. Candidates

Two views of the envelope are combined, because each has a failure the other
does not.

The **autocorrelation** of the mean-subtracted envelope, normalised by its
variance, peaks at the beat period and at every multiple of it. It also has
a peak at three halves of the beat period: a grid at that spacing lands on
kick, hat, kick, hat, and hats carry a lot of spectral flux. Tracks came
back at exactly two thirds of their tempo for that reason.

The **Fourier magnitude** of the envelope at a candidate rate peaks at the
beat rate and at every multiple of that (twice the tempo, three times). It
has no line at two thirds of the tempo, because a periodic signal has no
component below its fundamental. It is averaged over 20-second Hann windows
rather than taken over the whole track: a five-minute sum resolves 0.2 BPM,
so a candidate read off the autocorrelation a tenth of a BPM from the truth
drifts through most of a cycle and cancels itself. That was a real failure
(a 130 BPM track reading 86.6) before the windowing.

Every local maximum of the autocorrelation inside 70–200 BPM becomes a
candidate (parabolic interpolation of the peak), and so does each
candidate's ×2, ×½, ×3⁄2, ×2⁄3, ×3, ×⅓, ×4⁄3 and ×¾ that falls in range, so
the octave a listener would choose is in the running even when it is not
the strongest correlation. Each is scored

```
score = acf(bpm) × √fourier(bpm) × prior(bpm)
```

with `acf` and `fourier` each normalised to the strongest candidate, and the
prior a log-normal centred at 132 BPM with width 0.5 in log-ratio. The
Fourier term enters as a square root: its job is to rule out a period with
no spectral line at all (a hundredth of the strongest), not to prefer the
hat rate over the beat, where it is louder by half. The prior only breaks
ties between octaves.

Octaves are a convention. Drum & bass with the kick–snare backbone at 87
comes back at 174, which is what rekordbox does and what the golden
playlist demands; a 92 BPM pattern with heavy off-beat hats comes back as
185 by the same reading. Nothing below 123 BPM is in the playlist, so the
slow end is untested against rekordbox.

`tempo_candidates` exposes the scored table so a rig can print it for a
track that chose wrongly. `confidence` is how far the winner's prior-free
score stands above the best candidate that is not a simple ratio of it.

### 2. Fit

The winning BPM is only good to an envelope sample. A grid needs much more:
at 130 BPM, 0.05 BPM of error is 115 ms of drift over five minutes, a beat
and a half by the end of the track.

- **Period.** A comb filter — the envelope summed at every beat of a
  trial period, normalised per beat, at its best of 32 phases — is
  maximised over fractional periods: a sweep of ±1 sample in 0.02 steps,
  then ±0.03 in 0.002 steps around the winner.
- **Phase.** The comb is evaluated at 64 phases across one period and the
  best is sharpened by a parabola through its neighbours.
- **Snap and refit.** Every predicted beat is snapped to the highest
  envelope sample within ±20 % of a period (position sharpened by a parabola
  through its neighbours), and a line is fitted through the snapped beats by
  least squares, weighted by each peak's height. Beats whose peak is under a
  fifth of the median are left out altogether: an intro of pads has a
  noise-floor peak near every predicted beat, and a minute of those tilts a
  line that is then extrapolated back across the same minute. The line is
  fitted twice, the second time without the fifth of beats furthest from the
  first line (and never with any beat over a tenth of a period off it): a
  section of swung or late-hitting percussion snaps its beats consistently
  off the grid and, left in, bends a track's tempo by a hundredth of a BPM.
  Three passes.

Six hundred beats average a 5.8 ms hop down to a fraction of a millisecond.
On the golden playlist the beats land within +1 ms (median) of rekordbox's,
p10 to p90 spanning 0 to +3 ms.

### 3. Segments

DJ edits jump tempo mid-track, and rekordbox's grid for such a track is
piecewise constant: one tempo, then another, the beat count continuing
1–4 across the join. A single line through such a track is wrong on both
sides.

The tempo is measured in 16-second windows, hopping 8 seconds: the
autocorrelation of the window alone, its strongest peak folded onto the
octave of the track's tempo. A window where the track's own period still
correlates at 60 % of the best peak is not a change (a breakdown that keeps
only the hats is not a tempo change). The ratios that at least three
windows agree on, that differ from the track's tempo by over 2 %, and that
are not a ratio a rhythm produces on its own (3⁄2, 2⁄3, 4⁄3, 3⁄4 — a
dotted-eighth delay or a triplet feel) become the track's tempos; every
window is assigned to the nearest, a lone window between two of the other
tempo is absorbed, and an intro that says nothing belongs to the first tempo
heard rather than the track's. Each run is fitted on its own.

The boundary between two fits is placed where rekordbox places it: not
where the new beat first appears, but where it arrives. In a DJ edit the
next track's beat comes in under the last one's breakdown at a third to a
half of its eventual level for a few bars; the change goes at the first beat
of the new grid that starts four beats in a row each at least 75 % of the
median beat of its stretch, and whose next two bars are at least as
supported as the old grid's would be (two grids at nearby tempos drift
through each other, and the new one claims the old one's onsets for a few
beats every cycle). Measured on `Go Back [136-174]`, this lands on
rekordbox's change to the millisecond.

The first beat of the first segment is the first grid position at or after
the start of the file — the grid is extended back to time zero, which is
where rekordbox starts it too (a track whose first kick is on the downbeat
gets a first beat at ~24 ms, the MP3 encoder delay).

## Downbeat (`downbeat.rs`)

The tempo stage numbers beats 1–4 from the first one it found, which is a
claim about the beat and not about the bar — and its beat can be half a beat
off, because in a bar of tech house the open hat between the kicks carries
as much spectral flux as the kick, and in a bar of hardstyle the reverse
bass does. No band of the spectrum tells the kick from the off-beat on every
track: on the golden playlist the full band follows the hats on 22 tracks,
a 0–200 Hz band follows the bass on 19, 0–4 kHz still misses 8, and a
"kick coincidence" (low-band transient × mid-band transient) misses 54.

Structure does. Dance music changes at bar boundaries and, far more
strongly, at phrase boundaries eight or sixteen bars apart — a drop, a
breakdown, a new bass line, a filter opening — and every such change lands
on a downbeat, which is a kick, never on the off-beat.

- The track is summarised as one **profile per half beat**: mean log energy
  in twelve log-spaced bands from 40 Hz to 10 kHz, from a 2048/512 STFT,
  over the frames the half beat spans.
- At every half beat, at each of four **scales** (one, two, four and eight
  bars), the **novelty** is the Euclidean distance between the mean profile
  over `scale` half beats before it and over `scale` from it (prefix sums
  make each mean a subtraction).
- Only **peaks** of the novelty count. Summing the novelty itself put nearly
  equal weight on every position (a 3 % margin between best and next),
  because a slow crescendo raises every half beat alike; keeping local
  maxima raises the margin to 2.8×. Each scale's peaks are normalised to sum
  to 1, so a long phrase counts as much as a bar.
- The eight positions in the bar are scored by the peaks that land on them.
  An odd winner means the beats are on the grid's midpoints: every segment
  is moved by half a period. The winner's time is the downbeat; the beat
  nearest it becomes 1 and the count runs on from there.
- A tempo segment of at least 64 beats is then asked again on its own beats
  and renumbered from its own downbeat: a DJ edit's two halves are two
  pieces of music.

On rekordbox's own grids this picks rekordbox's downbeat for 153 of the 155
golden tracks (`golden downbeat`).

## Key (`key.rs`)

Spectral energy is folded onto the twelve pitch classes, then correlated
against major and minor profiles; the best of the 24 rotations is the key.

- **Chromagram.** An 8192-sample Hann-windowed FFT every 4096 samples
  (5.4 Hz bins, enough to separate semitones down to about 90 Hz). Each bin
  from 55 Hz to 2 kHz contributes its magnitude, split linearly between the
  two nearest of 36 sub-bins per octave (three per semitone). Each bin also
  credits the sub-bins of f/2, f/3 and f/4 with weights 0.6, 0.36 and 0.22,
  so a note's harmonics — which land on the fifth and the major third —
  vote for the note. The frames are summed, and each class is its centre
  sub-bin plus a quarter of each neighbour.
- **Matching.** Pearson correlation against Faraldo's `edma` profiles (fitted
  to electronic dance music), with 0.3 added to every minor key: 89 % of the
  library is minor, and a correlation alone cannot know that.
- **Tuning.** The sub-bin structure allows the chroma to be centred on a
  track's actual tuning; it is off by default because it changed nothing on
  the golden playlist, which is at concert pitch throughout.

An earlier version added `ln(1 + |X|)` for every bin near a semitone, which
made a pitch class's total mostly the number of bins that happen to round to
it, a fixed pattern that read most of the golden playlist as C minor
(27 of 155 right).

The shipped configuration is the best of 3,600 combinations searched by
`golden key`: **130 of 155 exact** (84 %), 138 within a fifth or the relative
key. Of the 25 misses, 11 are the same tonic in the other mode (mostly tracks
rekordbox calls major; rekordbox itself calls `Dolce (Extended Mix)` D and
`Dolce (Extended Instrumental Mix)` D minor), 8 are a fifth away, 6 are
elsewhere. Each of these was tried and did not move the count: a mode
decision from the third above the tonic, a separate smaller bias for the
mode, the tuning estimate, a bass-only chroma added in, per-frame
normalisation, high cut-offs of 1, 1.5, 3 and 5 kHz, three to six
harmonics, and profiles learned from the playlist itself with each track
held out (132 at best, within noise of 130). Rekordbox's key is our
runner-up on 14 of the misses and third on 6.

## Waveform (`waveform.rs`)

150 columns per second, each holding the peak of a low (< 200 Hz), mid and
high (> 2 kHz) band from one-pole crossovers, and the overall peak. This is
the decomposition the ANLZ colour waveforms encode, so the columns map onto
`PWV4`/`PWV5` directly.

## The golden gate (`examples/golden.rs`)

The measurement every change is judged by. The playlist `RBX-BPM-GRID-TEST`
in the installed library (155 tracks, all 4/4, 123–178 BPM) is the reference;
the rig is read-only against it.

```
cargo run --release -p rbl-analysis --example golden -- cache      # decode once
cargo run --release -p rbl-analysis --example golden -- eval       # score all
cargo run --release -p rbl-analysis --example golden -- eval <title substring>
cargo run --release -p rbl-analysis --example golden -- downbeat   # bar position on rekordbox's own grids
cargo run --release -p rbl-analysis --example golden -- key        # search the key front end and matcher
```

`cache` decodes every track once and keeps the mono PCM beside what
rekordbox recorded — BPM, key and the `PQTZ` grid — under `target/golden/`
(3.6 GB). `eval` then scores every track in about five seconds, in parallel.
With `RB_LITE_CANDIDATES=1` and a title filter it also prints the candidate
table, the segments, the fit's passes, the half-beat position scores and
the first beats side by side with rekordbox's.

| metric | passes when | result |
|---|---|---|
| bpm | within 0.05 BPM of rekordbox's, no octave folding | **155 / 155** |
| downbeat | the offset between our downbeats and rekordbox's, modulo a bar, is within 25 ms | **151 / 155** |
| grid | ≥ 98 % of rekordbox's beats have one of ours within 25 ms carrying the same beat number | **146 / 155** |
| key | the same name rekordbox chose | **130 / 155** |

The starting point was 146 / 27 / 26 / 27.

### What the misses are

**Eleven tracks whose rekordbox grid sits 25 ms after the kick.** On 140 of
the 144 MP3s rekordbox's beats sit on the kick's attack as decoded by
symphonia (untrimmed, encoder delay included); on these eleven they sit
1105 samples later, so our beats are −24 to −26 ms off and the grid metric
fails on the tolerance edge: *See Me Now, technikore & weaver – diya,
Missing, Insane Stampede, Sky Fall, Concrete Jungle, Voltage, Goddess, Drift
Around Me, Angels, Force of Gravity*. This is not derivable from the audio
or the file. It is not the decoder: rekordbox bundles `libmpg123`, and that
library, driven from Python via `ctypes`, produces exactly symphonia's
timeline for every tagged file with gapless off, or exactly 1105 samples
*earlier* with gapless on — never later, and never per file. It is not the
Xing/Info frame (61 MP3s have one; 50 of them align), not the LAME tag's
CRC (all valid), not the encoder version (three ffmpeg 7 files align, four
do not), not the ID3 version, not `Analysed`, `AnalysisUpdated`, `MasterDBID`
or `DeviceID` in `djmdContent`, and not when the `.DAT` was written. The
eleven were most likely analysed elsewhere (a different rekordbox build or
platform) or had their grids shifted; re-analysing them in rekordbox would
tell.

**Two DJ edits.** `Go Back [136-174]` has its change placed at rekordbox's
millisecond, but our 136 BPM line is 0.02 BPM off across an intro with no
kicks (18 ms at the first beat) and the novelty puts the downbeat one beat
before rekordbox's throughout. `Bring Me Back to Life [138-150]`: the 138
BPM beat is gone from 32 s and the 150 BPM beat is present, quietly, from
64 s; rekordbox holds 138 until 105 s, where the 150 track drops after a
three-second break. No strength rule matches both edits.

**BATTERY OPERATED.** Rekordbox's beats fall where every band of the
spectrum and the phrase structure say the off-beat is; the grid is half a
beat from ours.

**They Want Your Soul.** The comb says 133.003 BPM and the snap-and-refit
pulls it to 132.986, which drifts 22 ms by the end; 86 % of beats match.
The regression is kept because it is right more often than the comb alone
(with the comb alone: 143 grid passes instead of 146).

## Rekordbox facts learned along the way

- Rekordbox's grid is on the untrimmed MP3 timeline: a track whose first
  kick is at sample 0 of the master has its first beat at ~24 ms, the LAME
  encoder delay plus decoder delay (1105 samples). Symphonia with
  `enable_gapless: false` matches it.
- Rekordbox's grids are constant-tempo to within 1–3 ms over a whole track,
  with a piecewise-constant grid (continuous 1–4 count, a new phase at the
  change) for a DJ edit.
- Rekordbox starts the grid at the first grid position after time zero,
  extended back from the music, even over silence or a beatless intro.
- Ten of the 155 tracks have the downbeat one to three beats after the first
  beat; the first beat is not the downbeat by default.
- Rekordbox bundles `libmpg123.0.dylib` (in `rekordbox.app/Contents/MacOS`),
  whose default output here is 32-bit float.
