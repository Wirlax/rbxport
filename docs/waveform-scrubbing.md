# Waveform scrubbing

Dragging a waveform left or right plays the audio under the playhead at the
drag's speed and direction. A low-pass filter follows the magnitude of that
speed: slow drags sound darker, and fast drags reveal more high frequencies.
It applies on every scrub, regardless of whether the music contains a kick.

The cutoff follows a concave quadratic ease-out curve from 400 Hz at rest to 19 kHz
at normal playback speed, then stays at 19 kHz for faster movement. The curve
opens quickly at low scrub speeds and eases smoothly into its maximum as it
approaches 1x. Speed is measured in audio traversed per second, so waveform zoom
affects how much audio a given mouse movement crosses. Forward and reverse drags
use the same cutoff curve.

The engine uses a stereo, two-pole Butterworth low-pass driven by the smoothed
scrub playback rate. Filter coefficients glide with a 5 ms time constant to
avoid abrupt changes. Each output device's sample rate is used, with the cutoff
capped at 40% of that rate. The stop fade follows the filter, keeping a stationary
hand silent. Releasing the waveform discards the scrub filter; normal playback
uses its usual audio path.

Implementation: `crates/rbl-deck/src/scrub.rs`, with the device sample rate supplied
by `crates/rbl-deck/src/deck.rs`. Tests cover frequency response at 44.1, 48 and
96 kHz, direction symmetry, stereo isolation, cutoff smoothing, stability at low
sample rates, and the existing scrub motion and stop regressions.
