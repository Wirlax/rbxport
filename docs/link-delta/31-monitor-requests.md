# 31. Handle Link monitor and reset messages

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] rekordbox receives 1f/20 on UDP 50004; RBX has no monitor receiver.

## Task

Trace the vendor callback and packet layouts, then add only the evidenced monitor/reset behavior and response timing.

## Completion evidence

For valid, invalid, and wrong-state monitor packets, compare exact responses, state changes, and timeouts with the reference; preserve basic discovery.

## Sources and limits

V4 startRecv; V5 Monitor; R2 beacon startup.

A listening socket alone is insufficient for parity.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **receiver state transitions established; monitor response blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V5 `Monitor::messageReceived` (1072–1162) requires magic and
byte 0x1f <= 1. Kind 0x1f clears monitor flag/counter at +0x270/+0x278;
it is not a global Link reset. Kind 0x20 reads BE32 counter at frame 0x24
and two bytes at 0x28/0x29 (`PSvLinkMonitorRcvInfo::setData`,
3304–3321). Changed parameter bytes produce local message 0x31 and update
the cached pair.

Counter processing is gated by both local enable fields +0x274/+0x280
being 1. Initial enabled state establishes a baseline; only the explicit
small positive counter-gap branch synthesizes intermediate requests.
Other cases, including large jumps, follow the direct counter path.
The callback receives local message 0x32; V3 `linkProc` calls
`LinkMonitorReqReceived`, while 0x31 calls `noticeLinkMonitorInfo`.
No complete wire response is constructed by this receiver.

[OBS] R2 has no UDP-50004 monitor listener or corresponding state machine.

### Unknowns and bounded evidence attempt

Read the counter routine, parser and callback switch, searched supplied
exports for consumer definitions, and checked the link-export capture:
no UDP-50004 packets. [UNKNOWN] reply format/destination, counter rollover,
enable-state ownership, timing and client follow-up. Evidence task:
recover both callback bodies and capture reset, first request, small gap,
duplicate, rollover and disable sequences. Do not infer audio/monitor
payloads or ACKs from the local message sizes.

### Implementation handoff

Affected: R2 new bounded monitor codec/service state, R1 service lifetime,
and the downstream monitor consumer only after its contract is known.
Dependencies: 20, 25, 32 where transport shares lifecycle.
Fixtures must assert silence while disabled, exact parameter-change events,
counter transitions and outgoing bytes/destinations from verified evidence.
Smallest prerequisite: `RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
A fake-clock/unit receiver model alone cannot satisfy monitor parity.
