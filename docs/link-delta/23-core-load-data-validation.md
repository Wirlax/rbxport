# 23. Prove the basic track-load data sequence

Priority: P1 evidence gate. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] Waveform/grid/extended-cue/tagged-analysis routes exist; full load payload parity is unknown.

Direct consequence and boundary: Depends on 13–15. Do not fabricate account data or assume all 160 user-info bytes must match for basic desktop loading.

## Do this one thing

Compare one complete device-requested load sequence on representative controlled tracks, including existing user-info and delivery-info routes. Fix demonstrated payload/count/unavailable differences.

## Evidence and code

V4/V6 analysis and song-info helpers; R3 analysis/user-info/delivery dispatch; R4 blobs/catalog. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)
- [crates/rbl-link/src/blobs.rs](../../crates/rbl-link/src/blobs.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Cover waveform/grid, extended cues with comments/loops/colors, analysis tags, artwork, and absent data for the actual client sequence. Distinguish vendor callback account data from fields genuinely required by the client.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

