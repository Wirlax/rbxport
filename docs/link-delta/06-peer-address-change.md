# 06. Update a known player's address consistently

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] A keepalive can update RBX's peer address but leave Player.address old; rekordbox replaces changed membership.

Direct consequence and boundary: A real command-delivery failure has not been reproduced; the stale stored address is demonstrated.

## Do this one thing

Make the player and peer identity/address update coherent on an evidenced IP/MAC change, including stale greeting state as established by tracing.

## Evidence and code

V1 readConfigNotify; R2 keepalive updates. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Announce the same identity from a new address; verify both tables and the destination used by subsequent commands. Cover changed MAC, unrelated identities, and repeated keepalives.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **membership replacement established; greeting wire policy unresolved**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `readConfigNotify` (7402–7451) in states 6/8 compares
  existing member IP and all six MAC bytes against the keepalive. On a
  change, it removes/notifies the old member, then writes the new IP, MAC,
  type and flags, adds/notifies the member, and restarts ageing. Unchanged
  identity refreshes membership without that remove/add pair.
- R2 `DeviceTable::observe` (1027 onward) updates IP/name/type by
  number, but stores no MAC. `beacon::hear_announce` (767–784)
  initializes `Player.address` from the UDP sender only when inserted,
  then updates name/time only. R1 `Link::load_track` selects that player;
  the stale address therefore reaches the command path.
- Decode a valid foreign keepalive; update peer/player identity coherently.
  Preserve other logical members sharing the old IP. No direct vendor reply
  is proved by the replacement body. [ASSUME] RBX may use a replacement
  helper to clear stale loaded/playback fields; prove intended local state
  with regression assertions rather than describing it as vendor UI behavior.

### Unknowns and bounded evidence attempt

Read the complete replacement block and both RBX stores plus address-based
greeting cleanup. The source proves payload-IP/MAC replacement; it does not
prove a new greeting packet after replacement. The steady-state captures
supply no address-change sequence. [UNKNOWN] handling when payload IP
differs from UDP sender, MAC changes while IP stays fixed, and the vendor's
greeting follow-up. Evidence task: trace replacement through vendor member
notifications and capture a same-number IP/MAC change. Do not invent a
greeting sequence or a vendor anti-spoof policy. Those extensions remain
blocked; correcting the stale address for matching payload/sender is bounded.

### Implementation handoff

Affected: `beacon.rs`, `rbl-prolink::Peer/DeviceTable`, command-destination
tests in `rbl-link/tests/beacon.rs`. Dependencies: coordinate number-aware
removal with 05 and logical synthesis with 26. Fixtures: same number/new
matching payload+sender address; changed MAC; repeated identical keepalive;
unrelated identities and shared-IP survivors; assert both tables, greeting
ownership, and actual destination of a later load command.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`;
use fixture libraries if exercising catalog-backed load behavior.

## Step 2 — implementation (2026-10-05)

Implemented the bounded matching-payload/sender address correction.
Peer records now retain and refresh the six-byte MAC. Existing Player records
update address/type when the keepalive payload IP matches the UDP sender,
preserving other identities. An old greeting is forgotten only after its last
player/peer owner has moved. Existing greeting dispatch is unchanged.

State tests cover repeated IP/MAC replacement, both stores and shared-address
survivors. A real loopback UDP fixture constructs a Beacon with the changed
member, invokes `load_track`, and verifies the complete `19` command reaches
the new destination from the existing command socket.
`RB_LITE_TEST=1 cargo test -p rbl-link --lib` passed.
Payload/sender disagreement, same-IP MAC replacement lifecycle and vendor
greeting follow-up remain the Step 1 evidence tasks; loaded/playback fields
were not reset on an assumed lifecycle policy.

### Review remediation — F2 (2026-10-05)

The V5 `messageReceived` guard (1354–1367) now precedes membership/address
updates, not only rejection dispatch. After NetIF initialization, own-address
and off-subnet keepalives cannot mutate the peer/player address, MAC, greeting
ownership or command destination. Accepted same-subnet address corrections
retain the matching-payload/sender boundary above. Regression results are
recorded in [review.md](review.md); disagreeing addresses and vendor follow-up
remain unresolved.
