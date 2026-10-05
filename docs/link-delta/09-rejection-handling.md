# 09. Respond to an announcement rejection

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox handles 09 by sending 08 and dropping members; RBX ignores it.

Direct consequence and boundary: Do not assume rejection handling and collision handling have identical retry behavior.

## Do this one thing

Implement the evidenced rejection packet validation, state guard, outgoing disconnect, and membership/rejection state changes.

## Evidence and code

V2 readReject; R2 announcement dispatch. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Replay a valid rejection and check outbound bytes/state; cover wrong target, invalid packet, and applicable vendor states. Confirm later negotiation behavior from the reference before asserting it.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

