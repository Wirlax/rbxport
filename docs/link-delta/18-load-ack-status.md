# 18. Distinguish load-command response statuses

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox parses MusicDD response information; RBX logs every 1a as accepted.

Direct consequence and boundary: Status meanings must be recovered before implementation; callback forwarding alone does not prove enum semantics.

## Do this one thing

Parse and validate the evidenced status fields; report acceptance only for an established acceptance value. Unknown statuses must remain explicit, not treated as success.

## Evidence and code

V5 NormalInterval/MusicDDResInfo::setData; R2 LOAD_TRACK_ACK branch. Primary artifacts: [V5](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Feed known accepted/rejected responses, unknown values, and truncated packets; check reported state/logs. Verify a later status packet—not ACK alone—establishes actual load completion.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **field extraction established; status enum meaning blocked**.
[Evidence key](investigation.md).

### Contract and current coverage

[OBS] V5 `PSvLinkMusicDDResInfo::setData` (1375–1405) consumes a
40-byte `1a` packet: common header, big-endian payload length at
`0x22`, and bytes `0x24..0x27`. V5 `NormalInterval::messageReceived`
(515–526) forwards bytes `0x24/0x25` as an eight-byte local message
`0x22`. Those are the fields to preserve, not a blanket success event.

R2 `status_loop` (922–924) logs every magic-valid kind `1a` as
"accepted" without decoding the remaining length/fields. Add bounded
parsing; retain numeric unknown values. An ACK must not mark a track loaded
or playing. Later player status is the separate source for those states.
No network response to the ACK is established.

[OBS, capture] `push-load-cdj3000-emu-20260913.pcap` frame 37,
`192.168.1.152 → 192.168.1.14`, is exactly 40 bytes:

```text
5173707431576d4a4f4c1a43444a2d33303030000000000000000000000000010003000403010000
```

It contains `03 01 00 00` at `0x24..0x27`, following a kind-`19`
request in frame 36. This single response is not an enum definition.

### Unknowns and bounded evidence attempt

Read the parser and local-message construction and extracted the full
historical ACK above. V3 `InnerLinkAPI::linkProc` (325–330) routes local
0x22 to `musicDragDropResponse`; a search across the supplied C exports
found its calls but no body. [UNKNOWN] authoritative acceptance/rejection meanings,
correlation with request/deck, and reserved-byte semantics. Evidence task:
trace the local `0x22` consumer or device's `1a` constructor and retain
one accepted and one rejected load case. Do not label status 1 "accepted"
solely because this historical request was followed by it. Semantic enum
implementation remains blocked; neutral numeric reporting is bounded.

### Implementation handoff

Affected: `rbl-prolink/src/lib.rs` typed ACK decoder and packet tests;
`rbl-link/src/beacon.rs` logging/state tests. Dependencies: none for
parsing; 23/24 for completed-load evidence. Fixtures: frame 37, every
truncated prefix, wrong kind/length, unknown values, mismatched deck,
repeated ACK, and subsequent status naming the actual content. Smallest
validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink`, then the beacon
test. Do not introduce a success UI from unproven values.

## Step 2 — implementation (2026-10-05)

Narrow implementation: `LoadTrackResponse` decodes the established
40-byte kind-`1a` response and retains all four raw payload bytes.
The status loop logs neutral numeric fields rather than claiming acceptance;
neither loaded nor playing state is mutated by this response. Bad magic/kind,
truncation and an unexpected declared payload length are refused safely.

Tests retain the full frame-37 fixture, verify raw `03 01 00 00` and unknown
values, and reject every truncated prefix and wrong kind/length/magic.
A UDP Beacon regression sends captured, unknown and truncated responses,
uses a same-socket query as a processing barrier, and checks no loaded/playing
state or load event occurs; subsequent player status establishes the load.
Acceptance/rejection enum meanings, request correlation and client semantics
remain blocked. These are parsing/socket fixtures, not fresh device proof.

Validation: the focused
`RB_LITE_TEST=1 cargo test -p rbl-link --test beacon load_response_fields_do_not_establish_a_loaded_or_playing_track`
passed; `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon` passed
**6 tests, 2 existing ignores**. The initial barrier fixture retained the
captured query's asker IPv4 address, so its reply went outside the loopback
receiver; correcting that test payload to localhost resolved the failure.
No production response-destination or ACK behavior was changed for the fix.
