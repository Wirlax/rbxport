# 12. Allow original CDJ announcements on the evidenced wired path

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] RBX unconditionally excludes minor-0 CDJ-2000/CDJ-900 announcements; vendor exclusion is wireless-only.

Direct consequence and boundary: Actual compatibility of every old device is not established by removing this one gate.

## Do this one thing

Represent the evidenced connection mode and apply the model restriction only under the vendor condition. Trace how mode is determined before adding a boolean shortcut.

## Evidence and code

V1 linkUpFunc/readFrame; R2 brings_link_up. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test the same model/version in wired versus wireless mode, unaffected newer models, and unknown mode. Add a packet/integration fixture establishing that the wired eligible announcement advances Link.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

