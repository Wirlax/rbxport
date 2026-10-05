# 02. Decode and answer occupied-number bitmasks correctly

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] RBX compares a number field where rekordbox reads subtype-2 bitmasks and echoes a counter.

Direct consequence and boundary: No number-collision rate or hardware failure has been measured.

## Do this one thing

Replace the assumed subtype-2 interpretation with the evidenced wired/wireless masks and counter handling. Preserve ordinary subtype-0 probing as a distinct format.

## Evidence and code

V1/V6 readIdBlkRequset; R2 NumberProbe/Join::hear_probe. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Use packet fixtures for set/unset bits, wired and wireless ranges, nonzero counters, truncated requests, and subtype-0 regression. Check exact reply bytes and whether a reply is sent.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

