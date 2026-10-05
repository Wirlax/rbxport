# 06. Update a known player's address consistently

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] A keepalive can update RBX's peer address but leave Player.address old; rekordbox replaces changed membership.

Direct consequence and boundary: A real command-delivery failure has not been reproduced; the stale stored address is demonstrated.

## Do this one thing

Make the player and peer identity/address update coherent on an evidenced IP/MAC change, including stale greeting state as established by tracing.

## Evidence and code

V1 readConfigNotify; R2 keepalive updates. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Announce the same identity from a new address; verify both tables and the destination used by subsequent commands. Cover changed MAC, unrelated identities, and repeated keepalives.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

