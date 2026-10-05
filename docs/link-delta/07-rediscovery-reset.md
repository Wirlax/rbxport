# 07. Clear obsolete membership when a device rediscovers

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox removes matching stored MAC membership on discovery 00; RBX ignores it.

Direct consequence and boundary: Do not infer a universal MAC-only identity policy from this one branch.

## Do this one thing

Handle rediscovery using the evidenced MAC matching and state guards, clearing only the records that the vendor branch clears.

## Evidence and code

V1 readDiscovery/frameRead; R2 hear_announce. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Register identities, inject matching and unrelated discoveries, and assert member changes followed by successful fresh registration. Include shared-IP devices and malformed packets.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **type-scoped MAC removal contract established**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `frameRead` (6670–6830) passes `00` payload to
  `readDiscoveryRequest` (6832–7059). Discovery has counter at frame
  `0x24`, type at `0x25`, MAC at `0x26..0x2c`; require enough
  bytes before reading those fields. Outer enabled/compatibility/model
  gates still apply. The callback itself has no running-only state guard.
- Removal requires matching six-byte MAC and an active member **within the
  type-specific range**: type 1 checks IDs 1–4; types 2/3 check 33; type 4
  checks 17/18 and falls through to 41–44; type 6 checks 41–44; type 7
  checks 9–12, with paired 9→10 and 11→12 removal. Other types have no
  membership effect. Do not erase every member with the MAC regardless of type.
- Removed entries decrement count and notify membership (and clear the
  type-2 marker when relevant). No network reply or local rejoin is present.
  R2 `hear_announce` currently drops kind `00`; `Peer` has no MAC,
  so the required matching cannot be implemented from the current table alone.

### Unknowns and bounded evidence attempt

Read all type branches including type-4 fallthrough and type-7 paired
cleanup; scanned all five captures for announcements. No discovery packet
was available. [UNKNOWN] exact receiver length policy and resulting player
UI; a rediscovery trace is the next evidence task. Do not widen type 1 to
IDs 5/6 or turn this branch into a universal MAC identity rule merely because
newer devices can use additional numbers.

### Implementation handoff

Affected: `rbl-prolink/src/lib.rs` discovery decoder and MAC-bearing peer
state; `rbl-link/src/beacon.rs` and beacon tests. Dependencies: share
coherent removals with 05/06 and paired semantics with 26. Fixtures cover
every type/range, same MAC outside its range, different MAC at same IP,
inactive member, malformed discovery, and fresh keepalive registration
after cleanup. Preserve a greeting if another member at that IP survives.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink`, then
`RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.

## Step 2 — implementation (2026-10-05)

Implemented a bounded `00/00` discovery decoder and type-scoped MAC
membership cleanup. The shared removal helper updates both stores and greeting
ownership; only types/ranges listed in Step 1 participate. Type 7 preserves
the established primary→secondary removal rule. Rediscovery produces no wire
reply and does not reset the local Join state.

Complete synthetic discovery fixtures cover all listed type/range combinations,
same MAC outside the range, unrelated MAC at the same IP, unknown type,
nonzero subtype, every truncated prefix and fresh keepalive registration after
removal. Shared-IP survivors retain their greeting.
`RB_LITE_TEST=1 cargo test -p rbl-link --lib` passed.
Receiver length policy, synthesized OPUS ageing and client UI remain
unverified; this does not extend type 1 to IDs 5/6.

### Review remediation — F2/F3 (2026-10-05)

The shared V5 `messageReceived` guard (1354–1367) now rejects initialized
own-address/off-subnet discovery before MAC-scoped removal. Same-subnet
rediscovery keeps the established type/range rules.

V1 `readDiscoveryRequest` (6849–7049) removes membership without cancelling
its timers. The peer table now retains pending slot deadlines through that
removal and synthetic recreation, and consumes them even if a slot remains
inactive when they fire. This does not extend the six-second RBX timeout
policy or establish full OPUS lifecycle equivalence. See [review.md](review.md)
for the receive-context and removal/recreation regression results.
