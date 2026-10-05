# 16. Return browse type for the requested context

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox returns 1 for context byte 4 and 0 otherwise; RBX always returns 1.

Direct consequence and boundary: Client branch selection from the value still needs decoding/capture evidence; no specific screen failure is assumed.

## Do this one thing

Implement the evidenced context-dependent 3303 scalar reply while preserving the device-compatible supported query.

## Evidence and code

V4 OnOtherCmd; R3 BROWSE_TYPE. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Assert complete replies for context 4, other contexts, and malformed input; test the supported device browse path remains available.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

