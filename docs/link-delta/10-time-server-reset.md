# 10. Honor targeted time-server reset requests

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox handles 04 in specified running states by resetting members and number acquisition; RBX ignores it.

Direct consequence and boundary: This is a specific lifecycle request, not an instruction to implement every clock/sync feature.

## Do this one thing

Add the targeted request branch with the evidenced state guards and reset transition.

## Evidence and code

V2 readTimeServerRequest; R2 announcement dispatch. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test matching/nonmatching target numbers, running/nonrunning states, membership clearing, and restart of acquisition. Assert exact outgoing packets only when established.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

