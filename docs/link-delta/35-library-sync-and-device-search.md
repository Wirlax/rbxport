# 35. Handle device search, disconnect, and library sync messages

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor receiver matches DeviceSearch2, DeviceDisconnectReq/Res, GetLibrarySyncStatusRes, GetName, SyncStatus, SDP Link/HID, and related responses; RBX drops them.

## Task

Trace the reachable operations and their callbacks, then implement the request/response and state paths required by the declared full-parity scope.

## Completion evidence

Every inventoried message has an intentional RBX handler or a documented exclusion; captured search, sync, and disconnect scenarios match vendor packet and state behavior.

## Sources and limits

V5 NormalInterval and match helpers; R2 status dispatch.

Some matched packets may be responses to features RBX never initiates; determine reachability before writing handlers.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **receive families mapped; initiation/reply/sync contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V5 `NormalInterval::messageReceived` and named match helpers
identify DeviceSearch2 0x4c, device-disconnect request/result 0x6e/0x6f,
library-sync-status result 0x71, GetName 0x10, SDP Link/HID 0x12/0x13
and SyncStatus 0x29. They share magic/version checks, not a single
request schema. DeviceSearch2 is locally enabled and deduplicated by sender
IP; it forwards IP/name length/Unicode name as local 0x70.
Disconnect request/result post local 0x7e/0x7f, distinct from announcement
0x07/0x08. Library-sync result reads eight BE32 fields at 0x24..0x40
(`setData`, 2668–2730) and posts local 0x82 with status/state/progress,
inbound/outbound counters and file-download-wait.

SyncStatus filters source numbers (17/18, 41–44, 33) and updates cached
master/display/tempo state via local messages 0x49/0x47/0x4c.
V3 `linkProc` routes the search, disconnect, sync and SDP messages to
different consumers. R2 status dispatch does not implement that complete
surface; recognizing 0x29 in logging is not consuming its state.

### Unknowns and bounded evidence attempt

Read the dispatcher, match helpers, sync decoder and V3 callback routes.
The link-export capture includes periodic 0x29 but no 0x4c/0x6e/0x6f/0x71;
other supplied captures are not a full library-sync session.
[UNKNOWN] which result families RBX must initiate within the eventual
claim, outbound requests/replies, transfer ownership, disconnect cleanup and
sync persistence. Evidence task: recover initiating APIs and consumer bodies,
then trace device search/connect/sync/disconnect with both peers' state.
A received result without a reachable initiating feature may be an explicit
scope exclusion, never silently classified as unsupported request success.

### Implementation handoff

Affected: R2 status state/codec, R1 lifecycle, R4 catalog/file synchronization
only after fixture-safe write semantics are known. Dependencies: 25, 29–30,
33–34, 41–42. Fixtures: feature disabled/enabled, repeated search from same
IP, different device IDs, malformed lengths, all status codes, counter
progression, out-of-order results and reconnect. Assert exact responses or
silence and all callback/storage effects. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
Implementation remains blocked at the untraced callbacks, not at parser
recognition alone.
