# 30. Handle incoming beat and master handover traffic

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] rekordbox receives master request 26, response 27, and beat 28 on UDP 50001; RBX has no receiver there.

## Task

Trace the message manager and client interpretation for 26/27/28, then implement the needed listener, state transitions, and replies, including arbitration and timeout behavior.

## Completion evidence

Captured master takeover/release and beat sequences produce the same externally observable state, packets, and failure handling on a supported device.

## Sources and limits

V4 startRecv; V5 ShortInterval; R2 beacon/beat/master logic.

Forwarding to a vendor callback is not proof of the final handover outcome; finish the callback trace first.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **receive parsing established; arbitration/reply contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V5 `ShortInterval::messageReceived` (1026–1070) ignores the local
IP, requires the magic and byte 0x1f < 2, and dispatches 0x26/0x27/0x28.
The master-request parser reads a BE32 at 0x24 and posts local message
0x50; master response reads BE32 at 0x24/0x28 and posts 0x48.
Beat parsing consumes through byte 0x5f (96 bytes), converts its timing
words and posts local 0x52 only for the selected sender at frame byte 0x21.
Local message numbers are not wire replies.

[OBS] V3 `linkProc` routes these to `receiveSyncMasterReq`,
`noticeSyncMaster`, `receiveBeatInfo`; timer event 0x51 calls
`expmsTimer`. These consumer bodies are not in the searched exports.
R2's beat socket is an ephemeral transmit socket; `set_master` changes
local master state, not a receive/arbitration implementation on UDP 50001.

### Unknowns and bounded evidence attempt

Read the receiver/parsers and searched all supplied C exports for the
consumers. The link-export capture contains 72 kind-0x28 packets to 50001
and no 0x26/0x27; its 4042 kind-0x0b packets are a different family.
[UNKNOWN] arbitration state, rejection/timeout behavior, response bytes,
destinations and follow-up clock changes. Evidence task: export the four
consumers/timer and capture bidirectional handover with two masters and
timeout/rejection cases. Do not equate receipt with successful handover.

### Implementation handoff

Affected: R2 beat/status codec, receive socket ownership, MasterState and
app-facing status; maintain the existing IPC boundary. Dependencies:
12, 20, 25, 35. Fixtures: local/foreign/selected sender, truncated packets,
requests during each master state, competing requests, ACK failure and
timer expiry. Use fake time for arbitration plus loopback full-packet
tests, then a real handover trace. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-prolink`, followed by
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`. Parsing is not a complete
handover implementation contract.
