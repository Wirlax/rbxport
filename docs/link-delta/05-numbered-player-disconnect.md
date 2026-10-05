# 05. Disconnect the correct logical deck and its peer entry

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX removes all players at the sender IP and leaves a peer entry; rekordbox removes the payload-number member with specific paired-ID rules.

Direct consequence and boundary: Do not replace IP-wide removal with unconditional single-deck removal: vendor paired-ID behavior must be retained.

## Do this one thing

Resolve disconnects by the evidenced logical identity, maintain coherent player/peer/greeting state, and implement only verified paired-ID removal rules.

## Evidence and code

V2 readDisconnect; R2 hear_announce/remove_players_at. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Create two logical decks sharing one IP, disconnect one, and assert exactly the evidenced remaining members and cleared greeting/peer state. Cover 07/08 and special IDs separately.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **numbered removal contract established**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `frameRead` dispatches both `07` and `08` to V2
  `readDisconnect` (578–611, address `100f4c8bc`), passing frame
  payload at `0x24`. The first payload byte is the member number.
  Accept member IDs 1–80, vendor states 2–6 or 8, and an active named member.
  The callback does not match the sender IP to choose which member leaves.
- Mark that member removed and notify local membership change. For number
  9 remove 10 as well; for 11 remove 12 as well. No outgoing wire response
  or forced rejoin is present in this handler. A disconnect for 10 or 12
  does not remove its preceding member; an inactive primary is a no-op.
- [OBS] R2 `hear_announce` (723–727) instead calls
  `Shared::remove_players_at`, deleting all players at the UDP sender
  address and forgetting that address's greeting; `DeviceTable` survives.
  RBX needs removal by number in both tables. Preserve a shared-address
  greeting while a surviving logical member still uses it, consistent with
  the existing `expire_silent_players` greeting invariant (187–209).
  This last mapping is an RBX state-coherence requirement, not proof of a
  vendor greeting packet.

### Unknowns and bounded evidence attempt

Read the complete removal handler and dispatch and checked the available
announcement captures: no `07/08`. The static body consumes the number;
the exact vendor datagram-length acceptance and UI consequence remain
[UNKNOWN]. The vendor's outgoing disconnect constructor in V1 collision
handling is 41 bytes; it is a valid fixture, not proof that the receiver
requires exactly 41. Reject malformed/truncated input safely. Do not add
OPUS-wide four-deck deletion: this handler proves only the stated pairs.
A fresh trace is required to claim client-visible disconnect parity.

### Implementation handoff

Affected: `beacon.rs` shared removal/greeting helpers,
`rbl-prolink::DeviceTable` removal API and disconnect decoding,
`tests/beacon.rs`. Dependencies: no numbered prerequisite; coordinate
paired membership with 26 and avoid blocking ordinary numbered removal on
unproved extra synthesis. Fixtures: same-IP IDs 1/2; 9/10 and 11/12;
inactive/invalid IDs; every applicable state; both kinds; foreign sender;
truncation; surviving and last-member greeting cleanup.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`
and `RB_LITE_TEST=1 cargo test -p rbl-prolink`.

## Step 2 — implementation (2026-10-05)

Implemented payload-number removal in both the player and peer stores,
including only the 9→10 and 11→12 pairs. The dispatcher now reaches both
`07` and `08` (the latter is decoded as `AnnounceKind::Conflict`, rather
than `Other(08)`). Invalid/inactive IDs and idle/failed/absent Join state are
inert; accepted disconnects emit no wire reply and preserve acquisition state.
Greeting ownership is retained until the address's final logical member leaves.

Synthetic complete datagram/state tests cover both kinds, every prefix before
the consumed number, foreign sender, same-IP survivors, both pairs and reverse
(nonpaired) removals, inactive/invalid IDs, all discovery/probing/assigning/
running guards, idle/failed/absent state and final greeting cleanup.
`RB_LITE_TEST=1 cargo test -p rbl-link --lib` passed.
Vendor receiver-length policy and player-visible behavior remain unverified;
no OPUS-wide deletion was added.

### Review remediation — F2/F3 (2026-10-05)

Numbered disconnect now passes the shared V5 `messageReceived` sender guard
(1354–1367). With initialized NetIF, own/off-subnet `07` and `08` cannot
remove members. An unrelated same-subnet sender may still remove the member
named by the payload; no sender-owns-number requirement was invented.

V2 `readDisconnect` (589–608) removes membership without stopping the slot's
timer. `remove_number` now retains that pending deadline, including paired
removal. Synthetic recreation before expiry inherits the old deadline; a
direct keepalive refreshes it. An inactive slot's expired timer is consumed,
not saved for a later recreation. Whole-session clearing remains distinct.
See [review.md](review.md) for the focused timer and receive-context tests.
