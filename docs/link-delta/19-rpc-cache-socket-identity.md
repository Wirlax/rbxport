# 19. Keep duplicate RPC replies separate by receiving socket

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] Vendor duplicate-cache keys include socket identity; RBX's shared cache does not.

Direct consequence and boundary: No cross-socket player incident is proven. This closes a demonstrated cache distinction without redesigning the entire cache.

## Do this one thing

Include the local receiving service/socket identity in duplicate detection while retaining the established peer/request matching policy.

## Evidence and code

V7 duplicate cache; R5 cache/shared-server networking. Primary artifacts: [V7](../../../rbxport-private/verification/link/rekordbox-re/filsine.c).

Implementation locations:

- [crates/rbl-nfs/src/lib.rs](../../crates/rbl-nfs/src/lib.rs)
- [crates/rbl-nfs/src/net.rs](../../crates/rbl-nfs/src/net.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Send an identical request prefix from one source endpoint to distinct service sockets and assert independent processing. Verify true retransmissions on one socket still replay the proper reply.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

