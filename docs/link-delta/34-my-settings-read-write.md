# 34. Serve My Settings reads and writes

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor NormalInterval recognizes MySetting read/write, DJM MySetting read/write, and device-setting read/write; RBX has no equivalent dispatch.

## Task

Trace each request to its callback, data schema, authorization/context, storage, success/failure reply, and notification. Implement only validated fields and refuse unsupported writes with the correct envelope.

## Completion evidence

Controlled settings survive the evidenced read/write/reconnect sequence; invalid fields and unsupported versions receive client-consumable errors. Confirm settings UI behavior on the claimed device.

## Sources and limits

V5 NormalInterval/matchers; R2 status dispatch.

Do not store opaque or guessed settings bytes as a claimed successful write.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **request families and one captured read established; settings writes blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V5 NormalInterval dispatch and match/setData helpers distinguish
MySettings read/write 0x35/0x37, DJM settings 0x42/0x44, and device settings
0x46/0x48. MySettings and device read forward frame bytes 0x24/0x25 into
local messages 0x45/0x60. MySettings write requires length > 0x4f and
builds local 0x46 data of length 0x51; device write builds local 0x61 data
of length 0x24. DJM read checks its common header and length >= 0x28;
DJM write requires 0x34 plus `DJMMSData::getMinimumSize` and forwards
a distinct write-request object. These are different schemas, not one
opaque “settings” ACK. V3 `linkProc` (399–427, 513 onward) dispatches
their named read/write consumers. R2 status dispatch lacks these branches.

[OBS, capture] link-export frames 17731→17742 contain device-settings
0x46→0x47. The request originates at 192.168.1.152:48285 to
192.168.1.14:50002; the reply is sent from .14:50002 to .152:50002.
Full payloads are retained in the capture and shared inventory. This proves
one device-read success/destination, not MySettings/DJM writes or every
device settings value.

### Unknowns and bounded evidence attempt

Read dispatch, match/parsing helpers and V3 consumer calls; searched for
consumer definitions and enumerated available UDP-50002 kinds.
[UNKNOWN] complete setting field meanings/version gates, write validation,
persistence, error/ACK bytes and notification sequence. Evidence task:
export each consumer/serializer and capture read→write→read plus invalid
writes on an isolated profile, separately for all three families. No guessed
success response or fabricated default profile is authorized.

### Implementation handoff

Affected: R2 codecs/status handler and R1-owned settings state, with any
persistent settings routed through the existing backend boundary.
Dependencies: 25, 33 where settings travel via another transport, and 45.
Fixtures: each family/version, wrong target/context, short and invalid
settings, successful read, rejected write and persisted reread/restart.
Keep library/profile writes temporary. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-prolink`; then
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
The single 0x46/0x47 capture cannot unblock all six operations.
