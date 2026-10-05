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

## Step 2 — known-mode terminal rejection (2026-10-05)

Implemented the source-established Wired/Wireless branch, with Unknown
deliberately unchanged (09 remains silent and does not latch). Full issue
09 parity is therefore still blocked for Unknown classification and vendor
latch-clear/session-lifecycle recovery. Re-read V2 constructor (1–94),
`readReject` (614–741), V1 `linkUpFunc` (6463–6615), `frameRead`
(6670–6832), network broadcast send guard (6123–6143), and V5
`messageReceived` (1345–1376) before implementation. The
firmware-coverage-review checklist required distinct runtime identity,
database readiness, and exact failure-state envelopes.

| Known-mode state | Source-backed bounded behavior |
| --- | --- |
| Fresh Waiting, before any LinkUp attempt | Constructor configured/runtime number 17, runtime IP zero, NetIF unset. Valid 09 latches terminal Down and clears all stores, but the NetIF send guard permits no outgoing UDP. No running-only guard is added. |
| LinkUp attempted, including pre-running states | Cache the selected interface IP and candidate 17 (wired) or 41 (wireless). The wireless original-minor-0 attempt initializes these before failing its inner model gate and remaining Waiting. |
| Running | First rejection snapshots the assigned announcement number and runtime IP. Broadcast exact 08/00, 41 bytes, header 01/03, length 0029, number at 36 and IP at 37..40. |
| After rejection | Immediately zero the shared assigned-number cell, clear peer/player/greeting queues and master/runtime state, preserve selected NetIF IP/mask and configured candidate, and remain terminal. Repeated newer-model 09 sends cached candidate/runtime IP zero while the NetIF send guard still permits it. |

The selected interface's actual netmask is plumbed into `BeaconConfig`;
it is not reconstructed from broadcast. Initially on the new 09 path only, V5's
outer sender guard suppresses the equal NetIF address and off-subnet sender
after initialization; before that, no subnet restriction applies (the unset
zero address is still self). RBX requires a complete 36-byte common header
with valid magic/kind before rejection dispatch as a safe malformed-input
policy, not vendor length parity. Arbitrary subtype and declared length are
accepted; no payload target or new rejection-specific target guard exists.
Review finding F2 subsequently moved that same sender guard ahead of all
announcement dispatch, preserving rejection's cached-interface, fresh and
Unknown boundaries. The remediation and regressions are in [review.md](review.md).

Runtime original-model exclusion is independent of selected mode: initially
inactive (`0xff`), active after wireless classification, and active after
`readReject` clears the runtime byte to zero on either known mode. Original
minor-0 named repeats are consequently filtered before 09 dispatch; ordinary
newer-model repeats are not. See issue 12's implementation addendum.

The terminal latch guards subsequent keepalives, discovery, probes/blocks,
replies, and compatibility/reset processing. There is no collision-style
retry or latch clear on reset. The bounded membership/readiness work in
05/11 supplies these shared prerequisites; 04's unknown collision-retry
contract is not needed by this terminal branch and remains blocked.
A new Beacon/Join object clears the latch as
a bounded RBX constructor recovery policy, not a claim that vendor restart
lifecycle has been replicated. The assigned database gate is zero before
the caller can send the rejection output or open a new ready session;
already-open session behavior remains issue 11's unresolved boundary.
Late received player status is checked under the same Shared lock as
membership and `track_loaded`, so a stale pre-rejection number snapshot
cannot repopulate stores, greet, or report a load. A beat-clock completion
cannot restore a cleared runtime beat field. The old keepalive schedule is
disarmed even when the next tick observes an already-zero assigned cell.

Verification: eight focused rejection tests and one late-status test passed,
including exact independent byte fixtures, real broadcast-destination and
fresh/no-UDP announce-loop socket checks, both known modes in Waiting,
Discovery, Probing, Assigning, Running and Failed states, all cleared stores,
immediate database `open_ready` refusal, malformed header/magic, sender
guards, repeated cached-identity replies, original-model outer gates,
ordinary-frame/reset inertness, and fresh-object recovery. The captured
status fixture also has a positive fresh-object load-callback control.
`RB_LITE_TEST=1 cargo test -p rbl-prolink -p rbl-dbserver -p rbl-link`
passed 202 tests: prolink 35, DBserver 81, Link 86; two existing
environment-sensitive beacon timing tests remain ignored. All three crates'
doc-test targets passed (zero tests). Strict affected-crate all-targets
Clippy and `git diff --check` passed.
These are source-backed local codec/state/socket checks, not rejection
capture, booted vendor-firmware, or physical-device parity validation.
