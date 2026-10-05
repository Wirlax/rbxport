# 03. Honor keepalives while selecting a number

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox records occupied candidates during probing; RBX's keepalive occupancy path only handles Waiting.

Direct consequence and boundary: Depends on 02 for accurate probe-request behavior; a resulting collision is not itself proven.

## Do this one thing

Record evidenced occupancy from keepalives in the probing state so that an already-announced candidate is not selected through that path.

## Evidence and code

V1 readConfigNotify; R2 Join::hear_keep_alive. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Inject a candidate keepalive during probing without a probe reply; assert candidate rejection. Cover unrelated numbers, self-origin traffic, repeated announcements, and the Waiting path.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **state contract established; depends on 02**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `readConfigNotify` (7192–7315) validates member number
  1–80 and device type 1–9 before its state switch. In state 5 it ORs
  occupancy bits for 17,18,41,42,43,44 respectively into bits 0–5 of
  `this+0x1ac`; unrelated numbers do not alter that mask.
- Map that specific transition to R2 `State::Probing`: a valid foreign
  `06/00` keepalive marks the matching candidate in `in_use`;
  repeated traffic is idempotent, no immediate packet is returned, and
  later `tick`/`choose` must skip that occupied candidate.
  R2 `hear_keep_alive` currently acts only in `Waiting`.
- Preserve the waiting-state eligibility decision, the subtype-0 response
  path, and the existing self-origin exclusion in
  `beacon::hear_announce` (735–743). A valid self-echo must not mark a
  candidate occupied. Occupancy is not evidence that a DB session is ready.

### Unknowns and bounded evidence attempt

Read the whole state-5 branch and its initial guards, current `Join`
transitions and existing skip-candidate tests, and the shared capture scan.
[UNKNOWN] a real probing collision sequence: the captures contain only
steady-state keepalives. The bounded static contract is sufficient for this
state mutation; future packet/device proof must include a keepalive between
probe rounds without an accompanying `03`. Do not claim a measured
collision-rate improvement.

### Implementation handoff

Affected: `crates/rbl-link/src/join.rs`, beacon packet validation and
`tests/beacon.rs`; add codec validation only if necessary to enforce the
established keepalive shape/type guard. Dependency: 02's corrected probe
contract; shared mode handling from 12 for wireless claims. Use a controlled
clock and distinct MAC/IP fixtures, mark 17 during probing, then prove 18
is selected and 17 is never probed again. Test all six bits, repetitions,
unrelated number, wrong subtype/truncation, waiting behavior, and local echo.
No database fixture is needed.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link join::tests`,
then `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.
