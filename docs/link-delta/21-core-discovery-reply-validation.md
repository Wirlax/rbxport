# 21. Prove discovery-to-source-visible reply parity

Priority: P1 evidence gate. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX answers identity/media/property/handshake requests, but the report does not establish complete vendor reply parity or greeting triggers.

Direct consequence and boundary: This is a focused evidence task, not a mandate to remove RX3-specific replies or copy the vendor's entire settings receiver.

## Do this one thing

Trace the vendor reply paths and compare a single core discovery/source-selection sequence on a controlled fixture. Change only demonstrated mismatches in fields, destinations, state guards, or ordering.

## Evidence and code

V5 NormalInterval; V1/V3 lifecycle; R2 status/reply/greeting helpers. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c), [V3](../../../rbxport-private/verification/link/rekordbox-re/rb_bystring.c), [V5](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Record packet-by-packet vendor/RBX comparisons for 10/11, 05/06, 30/31, 16/17 where applicable. Record missing evidence as unknown; do not synthesize a universal ordering.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **comparison contract defined; acceptance evidence incomplete**.
[Evidence key and shared capture inventory](investigation.md).

### Required comparison and current coverage

Inspect each applicable request/reply as a separate row: UDP-50002 identity
`10→11`, media `05→06`, property `30→31`, handshake `16→17`,
plus greeting trigger and follow-up portmap/mount/DB connection. Retain full
request/reply bytes, source/destination endpoints, assigned identity,
readiness state and exact client/model/build. Do not infer universal order
from one device or count a receiver callback as a reply implementation.

[OBS] R2 `status_loop` (840–917) sends queued greetings after readiness,
identity/handshake to the sender's IP on configured player port, and routes
media/property through their helpers. `hear_announce` queues a first-CDJ
greeting; `hear_player_status` also has a greeting path. V5
`NormalInterval` is a parser/forwarder; the downstream response callback
must be traced before any claimed vendor field/state correction.

[OBS, capture] The link-export capture has media request frame 17599 from
`192.168.1.152:33189` to `192.168.1.14:50002`; reply 17604 goes
from `192.168.1.14:50002` to `192.168.1.152:50002`, not to the
ephemeral source port. Its full payload is retained in the original pcap.
It names device 17 and source slot 3. No `10/11`, `30/31`, or
`17` exists in that capture's UDP-50002 inventory. Two `16` packets
alone do not establish a matching `17` obligation.

### Unknowns and bounded evidence attempt

Enumerated all UDP-50002 kinds in link-export and browse-tour pcaps, extracted
media frames in full, and read the current dispatch and V5 receiver.
[UNKNOWN] remaining reply constructors, greeting guards, and applicability
of each pair per device. Evidence task: trace each callback and collect the
missing pairs on an isolated fixture with the same interface mode/build.
The older capture and current socket tests are not a matched vendor/RBX
acceptance run. Do not manufacture expected replies for absent packets.

### Implementation/evidence handoff

Affected: R2 packet helpers/`beacon.rs`, `tests/beacon.rs`, and a dated
comparison manifest. Dependencies: 02–12 and relevant 20 lifecycle guards;
01 does not prove discovery. Fixtures: waiting/probing/joined/reset states,
targeted vs foreign context, repeated requests, missing media, malformed
payloads and same-IP logical decks. Use literal captured bytes, with any
identity normalization explicitly recorded; assert destinations and next
client request separately.

Smallest local validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.
Acceptance requires a new paired trace and source-visible client result;
this investigation does not mark the gate passed.
