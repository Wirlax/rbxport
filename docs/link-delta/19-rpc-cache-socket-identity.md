# 19. Keep duplicate RPC replies separate by receiving socket

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

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

## Step 1 — investigation (2026-10-05)

Disposition: **cache-key contract established**.
[Evidence key](investigation.md).

### Contract and current coverage

[OBS] V7 transport receive (3360–3423) stores the receiving socket in the
first word and receives the peer sockaddr at +8. Its
`_tkfCheckTransportDataCache` (3426–3454) compares that socket, peer port
at +10, IPv4 at +12, and the first 24 request bytes. Twenty cached entries
are searched. This directly establishes the local-socket distinction.

R5 `ReplyKey` (202–210) currently has peer IPv4/port and request prefix
only. `net::serve` shares the Server across sockets and calls
`handle_from` without receiver identity. The same key can therefore
reuse a reply computed on another receiving socket.

For valid RPC datagrams long enough to form the prefix, include a stable
receiving-socket identity in cache lookup and insertion. True retransmission
on the same socket keeps current replay behavior and must not execute a
mutation twice; a different receiver must process independently. Replies
leave on the socket that received the request and target the original peer
endpoint. Preserve silent/uncacheable malformed-request handling and the
existing 20-entry bound. Do not redesign prefix matching in this issue.

### Unknowns and bounded evidence attempt

Read V7 receive/cache together to resolve the struct offsets, and R5 key,
shared Server and send loop. [UNKNOWN] an actual player incident caused by
cross-socket reuse; no incident is needed to justify the evidenced key fix.
A single-process fixture can expose the distinction without a device:
perform a cacheable mount request, remove/reset mount state through a
distinct request, replay the same original request on a second local socket,
and assert the side effect is recomputed there. Merely comparing two equal
NULL replies would not demonstrate independent processing.

### Implementation handoff

Affected: `rbl-nfs/src/lib.rs` handler API/key and `net.rs` call site;
RPC/socket tests. Dependencies: none; 43 uses the same transport. Use
temporary VFS exports, one source UDP endpoint and two distinct receiving
sockets; keep direct unit-test handler identity explicit. [ASSUME] a bound
socket token or local endpoint scoped to the Server lifetime is an RBX
implementation choice; avoid a token reused across rebinding in the same
cache lifetime.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-nfs`.
