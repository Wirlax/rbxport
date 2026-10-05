# 04. Reacquire a number when a joined peer claims ours

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox disconnects, clears membership, and retries after a running-state collision; RBX has no equivalent transition.

Direct consequence and boundary: Depends on 02–03. Do not invent the vendor random-delay distribution from its name.

## Do this one thing

Implement the evidenced running-state collision transition, including disconnect output and reset/retry state. Recover timer/state details before choosing a retry policy.

## Evidence and code

V1 readConfigNotify; R2 Join/beacon. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

With deterministic time/randomness, assert disconnect output, reset, and negotiation of an unoccupied number; verify unrelated keepalives and looped-back self packets do not trigger it.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

