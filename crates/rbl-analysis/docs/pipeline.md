# Target pipeline

The analysis as Chris wants it to run, drawn before it is built. Every box
is marked: **built** (in the crate today, unchanged), **changes** (exists,
but not like this), or **new**. Numbers in brackets are the current gate
results ([golden-gate.md](golden-gate.md)). The track is assumed to be in
4/4 throughout.

```mermaid
flowchart TD
    A[decoded audio] --> B[1. BPM and beat grid]
    B --> C[2. Beat 1]
    C --> D[3. Beat placement<br/>on the kick's attack]
    D --> E[4. Phrase starts]
    A --> F[5. Key<br/>Faraldo's edmkey]
    E --> F
    D --> F
    B & D & F --> G[Analysis]
```

## 1. BPM and beat grid

```mermaid
flowchart TD
    A[decoded audio] --> B[take the first 120 s]
    B --> C[onset envelope<br/>how much a hit happened, every 5.8 ms]
    C --> D[tempo candidates<br/>autocorrelation × √fourier × prior<br/>70–200 BPM; the faster octave wins when it carries the rhythm]
    D --> E[fit: exact period and phase<br/>comb, then snap to hits and refit]
    E --> F[extend the grid over the whole track]
    F --> G{tempo change?<br/>16 s windows over the whole track}
    G -- no --> H[one segment]
    G -- yes --> I[one segment per tempo, count carries on 1–4<br/>switch where the new beat is settled]
    H & I --> J[beats, numbered 1–4 from the first]
```

| box | status | note |
|---|---|---|
| first 120 s | **changes** | the BPM is read from the first 120 s only. Today the whole track is used. 120 s at 130 BPM is 260 beats, which the fit averages to well under 0.01 BPM, so precision holds |
| onset envelope, candidates, fit | built [155/155 BPM] | |
| extend over the whole track | **changes** | the grid is a straight line from the 120 s fit; today the line is fitted through the whole track |
| tempo change | built | this has to look at the whole track: a DJ edit changes tempo wherever it likes |

One thing to decide: a track whose tempo drifts (a live-played or
tape-sourced master) gets a worse grid from a 120 s fit than from a
whole-track fit. Nothing in the playlist drifts, so the gate cannot tell
the two apart.

## 2. Beat 1

```mermaid
flowchart TD
    A[beats] --> B[half-beat grid<br/>every beat and the midpoint after it]
    B --> C[one spectral profile per half beat]
    C --> D[novelty at 1, 2, 4 and 8 bars:<br/>drops, breakdowns, new bass lines, phrase starts]
    D --> E[peaks only]
    E --> F[score the 8 positions in the bar]
    F --> G{winner on a midpoint?}
    G -- yes --> H[move the grid half a beat:<br/>beat 1 is always on the kick]
    G -- no --> I[keep]
    H & I --> J[nearest beat to the winner is 1; count runs on]
```

All **built** [151/155]. This is the rule "the downbeat is where the music
changes" as it stands; beat 1 on the kick is what the half-beat move
enforces.

## 3. Beat placement on the kick's attack

```mermaid
flowchart TD
    A[a beat from the grid] --> B[band-pass 900–9000 Hz<br/>the click of the kick, not its body or the bass]
    B --> C[RMS in short steps, ~1 ms]
    C --> D[find the RMS spike nearest the beat:<br/>the biggest rise, within a few tens of ms]
    D --> E[the attack is where the spike starts]
    E --> F[backtrack to the previous zero crossing<br/>of the band-passed signal]
    F --> G[that sample is the beat]
    G --> H[refit the grid line through the placed beats]
```

| box | status | note |
|---|---|---|
| band-pass, RMS, spike, zero crossing | **new** | today a beat is the peak of the full-band onset envelope, sharpened by a parabola — on the attack to within ±3 ms of rekordbox on 140 of 144 MP3s |
| refit through placed beats | **changes** | the snap-and-refit exists; it would snap to these sample-accurate points instead of envelope peaks |

What this buys: a beat defined by the audio rather than by a 5.8 ms hop,
and a definition that can be checked by eye on a waveform. What to watch:
a zero crossing can sit a few ms before the spike on a slow attack, and
rekordbox's grids sit on the spike itself; the gate will show which of the
two rekordbox means.

## 4. Phrase starts

**Built.** The four-bar novelty peaks on downbeats, about six per track.
See [phrase.md](phrase.md). Phrase *labels* (intro, chorus…) stay
unimplemented.

## 5. Key — Faraldo's edmkey

Ángel Faraldo's method, as Essentia's `KeyExtractor` runs it. Every value
below is Essentia's default, taken from its source.

```mermaid
flowchart TD
    A[decoded audio, 44.1 kHz] --> B[frames of 4096 samples, hop 4096, Hann window]
    B --> C[magnitude spectrum]
    C --> D[spectral peaks: up to 60, 25–3500 Hz,<br/>magnitude ≥ 0.0001, interpolated between bins]
    D --> E[spectral whitening]
    E --> F[HPCP, 12 bins: each peak credits its pitch class<br/>and those of f/2, f/3, f/4 with weights 1, 0.6, 0.36, 0.22;<br/>cosine weighting one semitone wide; reference 440 Hz]
    F --> G[gate: bins below 0.2 of the frame's peak set to 0]
    G --> H[average over frames; detuning correction<br/>rolls the profile so its strongest bin is centred]
    H --> I[Pearson correlation against the bgate profiles,<br/>major and minor, at all 12 tonics]
    I --> J[best correlation = key]
    J --> K[rule pipeline: PreferMinor, …]
```

The `bgate` profiles (tonic first; note the zeros — "gated"):

```
major: 1.00 0.00 0.42 0.00 0.53 0.37 0.00 0.77 0.00 0.38 0.21 0.30
minor: 1.00 0.00 0.36 0.39 0.00 0.38 0.00 0.74 0.27 0.00 0.42 0.23
```

and `edma`, the profiles we use today (Essentia's rounding):

```
major: 1.00 0.29 0.50 0.40 0.60 0.56 0.32 0.80 0.31 0.45 0.42 0.39
minor: 1.00 0.31 0.44 0.58 0.33 0.49 0.29 0.78 0.43 0.29 0.53 0.32
```

`braw` (the same shape as `bgate` before gating) and `edmm` (a flat major
profile with an `edma`-like minor, i.e. "assume minor") are also in
Essentia.

| box | status | note |
|---|---|---|
| frames, spectrum | **changes** | we use 8192/4096; Faraldo uses 4096/4096 |
| spectral peaks, whitening | **new** | we credit every bin; Faraldo credits peaks only, whitened, which drops the noise floor between notes |
| HPCP with harmonics | built | our chroma folds f/2, f/3, f/4 down with the same weights; range 55–2000 Hz against his 25–3500 |
| gate at 0.2 | **new** | |
| average + detuning correction | **changes** | we average; our tuning offset exists but is off (measured no gain) |
| profiles | **changes** | we ship `edma`; `bgate` scored lower on our chroma, but has not been tried on his front end |
| rule pipeline | built | `PreferMinor` [89 → 130 of 155] |

The plan is to build his front end as a second `KeyOptions` preset and
let the rig (`golden key`) score it with each of the four profiles, with
and without `PreferMinor`, against today's 130.

## What is not in this picture

- The eleven rekordbox 6 grids, the two hand-made DJ-edit grids, and
  BATTERY OPERATED — see [golden-gate.md](golden-gate.md). None of the
  boxes above changes those.
- Phrase labels and vocal detection.
