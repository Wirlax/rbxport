# Analysis rules

The rules Chris has set for how this library's music is to be read, and
what the code does with each. They are general: none is about a particular
track. Evidence and measurements live in [golden-gate.md](golden-gate.md)
and [key.md](key.md).

## The reference

- **The playlist `RBX-BPM-GRID-TEST` is the truth.** Every track in it was
  analysed by rekordbox and its grid checked or set by hand.
- **Accuracy must reach 99 % on BPM, downbeat/grid and key** against that
  playlist.
- **Where rekordbox versions disagree with each other, follow the current
  one.** The decoder in `rbl-audio` reproduces rekordbox 7's timeline.

## Time signature and tempo

- **Every track is in 4/4.** A bar is four beats; beats are numbered 1–4
  and 1 is the downbeat. Nothing needs to detect a metre.
- **Drum & bass is counted at the fast tempo** (174, not 87). When two
  octaves are both plausible, the faster one wins if it carries the most
  rhythm at its rate.
- **A tempo change is a new segment**, with the beat count carrying on 1–4
  across the join, as rekordbox writes it.
- **A grid is set where the kick drums state the tempo reliably.** Through
  a stretch where they do not — a breakdown, a new tempo that is only
  hinted at by percussion — the grid holds the tempo it had and switches
  where the new tempo is settled and the kicks are reliable again.
- **A gradual tempo change between two settled tempos is gridded bar by
  bar.** Each bar's first downbeat is found by bisection: it lies between
  where the old tempo and the new tempo would put it, and the kick is
  looked for between those bounds. A cut goes at every such downbeat and
  each bar carries its own tempo, so a rise or fall — linear or not, up or
  down — is followed a bar at a time until the tempo is settled again.

## Downbeat

- **The first beat is not the downbeat by default.** Beat 1 is where the
  music changes: drops, breakdowns, new bass lines, the starts of phrases
  eight and sixteen bars apart.
- **Beat 1 is on the kick, never on the off-beat**, whatever the hats or an
  off-beat bass line are doing.

## Key

- **Key detection is Ángel Faraldo's edmkey method**, as Essentia's
  `KeyExtractor` runs it: spectral peaks, whitening, a harmonic pitch
  class profile, a per-frame gate, detuning correction, and his profiles
  fitted on electronic dance music. The rules below are applied after it.
- **A toss-up between a major key and its parallel minor goes to the
  minor.** (D or Dm: Dm.) The code's `PreferMinor` rule adds a fixed bias
  to every minor key's score.
- **When the key is not easy to determine, listen to the bass.** In the
  first 45 and the last 45 seconds, and on the first two bars of a phrase,
  the bass is usually playing the root note. Within each beat, listen on
  the second eighth: the kick, tail included, takes the first sixteenth to
  eighth of the beat, so the second eighth is the bass line alone. The
  code's `BassRoot` and `BassVote` rules read the bass in each of those
  windows; which of them ship is decided by measurement ([key.md](key.md)).
- Key names are rekordbox's: `Dbm`, `F#m`, `Abm`, `Bbm` for the minors and
  `Db`, `F#`, `Ab`, `Bb`, `Eb` for the majors, matching `djmdKey.ScaleName`.

## Adding a rule

A rule is a variant of `key::Rule` with its knobs, a match arm in
`key::apply`, and a line in this file, stated generally. `golden key` then
reports how often it fires, what it fixes and what it breaks, against the
shipped rules.
