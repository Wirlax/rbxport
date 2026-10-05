# 09. Respond to an announcement rejection

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox handles 09 by sending 08 and dropping members; RBX ignores it.

Direct consequence and boundary: Do not assume rejection handling and collision handling have identical retry behavior.

## Do this one thing

Implement the evidenced rejection packet validation, state guard, outgoing disconnect, and membership/rejection state changes.

## Evidence and code

V2 readReject; R2 announcement dispatch. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Replay a valid rejection and check outbound bytes/state; cover wrong target, invalid packet, and applicable vendor states. Confirm later negotiation behavior from the reference before asserting it.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **rejection contract established; do not reuse collision retry**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `frameRead` calls V2 `readReject()` for kind `09`,
  without passing a payload or target. V2 (614–741, `100f4c9bc`)
  has no additional numbered-target or running-only guard.
  The outer enabled/compatibility/model gates remain applicable.
  The proposed acceptance test for a "wrong target" must not invent a
  target field absent from this handler.
- Broadcast `08/00`, 41 bytes, current own number at `0x24`, own
  IPv4 at `0x25..0x29`, common header and length `00 29`.
  Set rejection latch `this+0x710=1`; remove/notify every active member;
  emit local state and rejection notifications (3 and `0x15`); zero
  runtime state `0x164..0x1b7`, restore configured number/type.
  There is no random-retry timer call here.
- [OBS] V1 `linkUpFunc` (6463 onward) first checks the rejection latch,
  so merely receiving a subsequent ordinary keepalive cannot rejoin through
  that path. R2 currently ignores `09` and has no equivalent latch.

### Unknowns and bounded evidence attempt

Read the full rejection body and the link-up latch guard; the five captures
have no rejection. [UNKNOWN] all operations that clear the latch and exact
receiver length policy. Evidence task: find all writes to `this+0x710`
in the exact vendor build and trace explicit stop/start/reset after rejection.
Implementing automatic retry would be a guess. A bounded implementation
may represent rejection as a terminal/down condition pending explicit
lifecycle recovery, but must not claim that recovery matches the vendor
until the latch-clear path is established.

### Implementation handoff

Affected: `join.rs`, `beacon.rs`, disconnect encoder in `rbl-prolink`;
app down reporting in R1 may need to preserve the rejection reason.
Dependencies: common lifecycle/membership clearing from 04/05/11, without
inheriting 04's retry policy. Fixtures: exact broadcast output, states before
and after rejection, emptied peer/player/greeting stores, repeated rejection,
ordinary keepalive after rejection, malformed header, and outer-gate silence.
No fake target field in tests.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link join::tests`,
then `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.
