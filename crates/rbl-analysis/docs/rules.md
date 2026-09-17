# Analysis rules

The rules Chris has set for how this library's music is to be read — in
his words where possible — and what the code does with each. They are not
something the audio or rekordbox could tell us. Where a rule has been
measured, the number is here too.

## The reference

- **The playlist `RBX-BPM-GRID-TEST` is the truth.** Every track was
  analysed by rekordbox — at different times, some by rekordbox 6 and some
  by 7 — and its grid checked or set by hand. `examples/golden.rs` scores
  against it ([golden-gate.md](golden-gate.md)).
- **Accuracy must reach 99 % on BPM, downbeat/grid and key.** On 155
  tracks that is at most one miss per metric.
- Where rekordbox 6 and 7 disagree with each other (eleven tracks whose
  grids sit 25 ms later than the rest), the code follows rekordbox 7, which
  is what the decoder in `rbl-audio` reproduces.

## Time signature and tempo

- **Every track is in 4/4.** A bar is four beats; beats are numbered 1–4
  and 1 is the downbeat. Nothing needs to detect a metre.
- **Drum & bass is counted at the fast tempo** (174, not 87). When two
  octaves are both plausible, the faster one wins if it carries the most
  rhythm at its rate. Nothing in the playlist is below 123 BPM; the slow
  end is untested.
- **A tempo change is a new segment**, with the beat count carrying on 1–4
  across the join, as rekordbox writes it.
- **A hand grid holds the old tempo until the new one is settled and the
  kicks are reliable.** Go Back is 136 until bar 61, rises (not linearly)
  until bar 82.2, and is 174 from there; the hand grid keeps 136 through
  the rise and switches at bar 82.25. Bring Me Back to Life is gridded
  where the kick states the tempo plainly and held through the stretch
  where it does not, even though percussion at the new tempo is already
  audible there. So the code places a change where the incoming beat is at
  full strength, not where it first appears. On Go Back that is the same
  millisecond as the hand grid; on Bring Me Back to Life the code still
  switches 40 s early.

## Downbeat

- **The first beat is not the downbeat by default.** Ten of the 155 tracks
  have beat 1 one to three beats after the grid's first beat. Beat 1 is
  where the music changes: drops, breakdowns, new bass lines, the starts of
  phrases eight and sixteen bars apart.
- **Beat 1 is on the kick, never on the off-beat**, whatever the hats or an
  off-beat bass line are doing.

## Key

- **A toss-up between a major key and its parallel minor goes to the
  minor.** (D or Dm: Dm.) The library is 89 % minor. The code's
  `PreferMinor` rule adds 0.3 to every minor key's score; it fixes 50
  tracks and breaks 9, taking key from 89 to 130 of 155.
- **When the key is not easy to determine, listen to the bass.** In the
  first 45 and the last 45 seconds, and on the first two bars of a phrase,
  the bass is usually playing the root note — and within each beat, on the
  second eighth: the kick, tail included, takes the first sixteenth to
  eighth, so the second eighth is the bass line alone. The code has this as
  the `BassRoot` and `BassVote` rules with a `source` for each of those
  windows. Measured on the playlist, the bass's strongest pitch class *is*
  rekordbox's tonic on 105 of 155 tracks when read on the second eighths
  (87 in the intro and outro, 99 in the first two bars of phrases), and as
  a fallback on close calls it fixes none and breaks up to nine, so the
  rules are in the pipeline but off by default. See [key.md](key.md).
- Key names are rekordbox's: `Dbm`, `F#m`, `Abm`, `Bbm` for the minors and
  `Db`, `F#`, `Ab`, `Bb`, `Eb` for the majors, matching `djmdKey.ScaleName`.

## Adding a rule

A rule is a variant of `key::Rule` with its knobs, a match arm in
`key::apply`, and a line in this file. `golden key` then reports how often
it fires, what it fixes and what it breaks, against the shipped rules.
