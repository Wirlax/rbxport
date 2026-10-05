# 33. Support the DeviceConnect and DeviceConnect2 stream lifecycle

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor has separate framed stream reassembly handlers; RBX's UDP 16/17 handshake is a different path.

## Task

Trace both stream protocols through connect, message dispatch, disconnect, and error callbacks. Add an RBX path only for the supported client types identified by that trace.

## Completion evidence

A client fixture replays complete, split, repeated, and truncated frames and obtains the evidenced replies and state. Record whether desktop/mobile clients require each path.

## Sources and limits

V4 DeviceConnect; V5 DeviceConnect2; R2 UDP handshake.

A UDP handshake reply does not establish stream or mobile parity.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **two receive state machines mapped; application/write contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `PSvLinkDeviceConnect::messageReceived` (1230–1623)
accumulates a 36-byte header, then kind-specific payload. Kinds 1/0x0a
use additional BE32 lengths; 5/7/0x0c consume path-like payloads;
8 changes a flag. Completion posts local messages 0x36/0x39/0x38/0x53/0x59;
null sender posts 0x40 error/FIN. Buffers and header state reset after
completion. This is not the UDP-50002 0x16→0x17 greeting.

[OBS] V5 `DeviceConnect2::messageReceived` (1186–1343) is separately
enabled and accumulates header/body for 0x15, 0x18, 0x1b, 0x1d, 0x1e.
`setFrmHdr`, `isContinue`, `setFrmData`, `messageRequest` and
`clear` (3399–end) establish variable segments, memory versus temporary
file storage, and cleanup. Parsed results post local 0x72 (connect with
IDs/account/name), 0x75 (track list), 0x7a/0x7b (file data/path),
0x7d (delete result) or 0x78 (send result). FIN posts 0x83.
These are received results, not authorization to expose inbound file writes.

[OBS] R2 implements the UDP greeting but neither stream receiver. V3
`linkProc` routes the local messages to distinct mobile/file callbacks.

### Unknowns and bounded evidence attempt

Read both complete receiver state machines and DeviceConnect2 segment/
completion/cleanup helpers; searched consumer definitions in the supplied
exports. [UNKNOWN] initiator/port negotiation, all derived segment layouts,
callback replies, session/account authorization, durable writes and rollback.
No supplied capture is labelled a mobile transfer scenario. Evidence task:
recover the derived receive-info schemas and request emitters/consumers;
capture connect, list, file transfer, cancellation and failure with an
isolated fixture account/library. A stream must not be advertised until
these contracts close.

### Implementation handoff

Affected: dedicated R1-owned stream service, R2 handshake integration,
R4 fixture-safe file/catalog boundary; do not write through arbitrary
peer-supplied paths. Dependencies: 25, 29, 32, 35, 41–42.
Fixtures: byte-at-a-time and concatenated messages, each kind, length limits,
truncation/FIN, temporary-file cleanup, partial failure and reconnect,
with complete outbound/state/storage assertions. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
Any storage exercise must use a temporary fixture, never the installed library.
