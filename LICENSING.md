# Licensing

The code in this repository is MIT, in [`LICENSE`](LICENSE).

**The application you build from it is GPL**, because of one dependency.

## Rubber Band

`crates/rbl-deck/vendor/rubberband` is the [Rubber Band
Library](https://breakfastquay.com/rubberband/) v4.0.0, vendored, and it is
GPL-2.0-or-later or a commercial licence from Particular Programs Ltd. This
project takes the GPL. Its own copyright and terms are in
`crates/rbl-deck/vendor/rubberband/COPYING`, unmodified.

Linking it makes the binary a combined work, so **anything distributed from a
default build is distributable only under the GPL** — source offer included.
MIT is compatible with that: this repository's own files stay MIT and can be
reused under those terms, but a shipped `rekordbox-lite` cannot.

It is here because a DJ deck needs a stretcher that holds a pitch. The WSOLA
backend written for this crate stretches tempo well and misses an interval by
25 to 50 cents, which is a key shift that puts a track in the wrong key.
Rubber Band R3 lands the same intervals inside 1.4 cents.

## Building without it

```sh
cargo build -p rbl-deck --no-default-features
```

The `rubberband` feature is on by default; off, the vendored sources are not
compiled and nothing links to them, `MASTER TEMPO` falls back to the WSOLA
backend, and the build carries no GPL obligation. Key shifting is not offered
in that configuration, which is what `Stretcher::shifts_pitch` reports.
