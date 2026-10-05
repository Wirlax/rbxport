# 32. Implement the system-manager TCP request path

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor SysMgrTCP listens on TCP 50000 and forwards checked frames; RBX exposes database TCP services instead.

## Task

Trace the frame parser, callback operations, lifecycle, and actual client connection sequence; implement the evidenced service without conflating it with RemoteDB.

## Completion evidence

Recorded TCP frames produce equivalent replies, state changes, close/error behavior, and reconnect behavior. Confirm client need with a capture or firmware path.

## Sources and limits

V2 SysMgrMainComponent constructor; V5 SysMgrTCP; R1/R3 sockets.

No universal TCP operation set can be inferred from the existence of a listener.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **TCP ingress behavior established; robust stream/reply contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V2 system-manager constructor (1–100) starts UDP and TCP listeners
on 50000 and installs the same callback at `SysMgrMainComponent+0xe0`.
V5 `PSvLinkSysMgrTCP::messageReceived` (1164–1185) applies
`checkFrameHeader` (3323–3396) before forwarding to that callback.
The header check accepts magic, byte 0x20 < 2, kinds 0x00–0x0b;
subtype must be zero except kinds 0x02/0x03 permit zero or one.
In particular, UDP ID-block subtype 2 is not accepted by this TCP check.

[OBS] V1 `PSvLinkTCPServer::startRecv/run` (1397–1580) binds IPv4
INADDR_ANY, listens with backlog 1, accepts one connection and forwards
each positive `recv` chunk (up to 1024 bytes) independently. EOF/error
closes the accepted socket. No reassembly/splitting is visible here or in
the inspected TCP callback. This is a source limitation, not proof that
arbitrarily fragmented frames work. The callback forwards into shared
system-manager state; it does not construct a TCP response itself.
R1/R3 expose RemoteDB TCP services, not this system-manager listener.

### Unknowns and bounded evidence attempt

Followed constructor → socket loop → header check → shared callback.
The link-export capture has no TCP-50000 session. [UNKNOWN] actual
fragmentation/coalescing behavior, peer identity in callback storage,
and whether responses go through the existing UDP sender or another path
for each opcode. Evidence task: capture a genuine TCP connection and trace
the shared callback/emitter for each accepted kind; test split/coalesced
frames against that build. Do not promise a framed TCP protocol merely
because the transport is a stream.

### Implementation handoff

Affected: R2 shared system-manager decoder/state and a dedicated bounded
TCP service owned by R1. Dependencies: 02–10, 20, 25, 27.
Fixtures: every allowed kind/subtype and forbidden subtype 2, short headers,
split/coalesced chunks, EOF, reconnect, concurrent connection and lifecycle
cancellation; record output transport and destination, not just payload.
Smallest prerequisite: `RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
Stream robustness may be an explicit RBX design decision, but is not yet
source-proven vendor parity.
