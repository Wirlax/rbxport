# 08. Expire silent peers without requiring more incoming traffic

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX peer expiry runs during keepalive reception; with no further keepalives that path cannot clean the map. Vendor membership uses timers.

Direct consequence and boundary: 10-second/180-second vendor thresholds are reported, but blindly applying one timeout to every RBX table is not justified.

## Do this one thing

Move the evidenced peer-ageing decision to periodic processing and establish the correct refresh sources and wired/wireless thresholds before changing constants.

## Evidence and code

V1 ageing/readConfigNotify; V2 constructor/timers; R2 timeout paths. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c), [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Advance a controlled clock with no new network input and assert expiry. Cover status-only traffic, keepalive refresh, multiple peers, and the distinction between player and peer records.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **periodic peer expiry established; broader timer parity gated**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V2 constructor initializes `this+0x15c` to 10000 ms.
  V1 `readConfigNotify` (7556–7582) restarts the active primary
  member's timer at that value for wired mode, multiplied by 18 for
  wireless mode. V3 `timerFuncAging` (3592–3653) stops the indexed
  timer, removes/notifies an active member and its 9→10/11→12 pair.
  Expiration does not depend on another network packet and emits no wire reply.
- [OBS] R2 `DeviceTable::expire` uses a six-second keepalive-only
  timestamp, but the sole call is inside `hear_announce` after a new
  keepalive. `announce_loop` does periodically call
  `expire_silent_players`; that separate player timestamp is refreshed by
  status and keepalive. It is not the peer membership timer.
- Invoke peer ageing from periodic processing with a controlled clock,
  refresh from evidenced keepalives, and keep player/activity state distinct.
  Member count in subsequent keepalives must reflect the expired peer map.
  Use 12's explicit mode before adopting mode-specific thresholds.

### Unknowns and bounded evidence attempt

Read timer initialization, keepalive timer restart, V3 timer callback, and
both RBX expiry paths. Capture scan contains ordinary keepalives but no
controlled silent-peer experiment. [UNKNOWN] whether downstream vendor
status callbacks can refresh membership and how OPUS extra synthesized
members age; the callback proves pairs only. Evidence task: trace status
delivery to SysMgr and timers for synthesized 11/12, or record keepalive-only,
status-only, and total-silence cases. Do not apply one timeout to every RBX
table or claim all vendor refresh sources have been excluded.

### Implementation handoff

Affected: `beacon.rs` periodic loop, `rbl-prolink::DeviceTable`,
deterministic expiry tests. Dependencies: mode config from 12 and synthesis
from 26.
The narrow periodic invocation can preserve current timeout semantics
while timer-parity work remains gated; that is not completion of the
broader issue. Fixtures need no incoming traffic while time advances,
boundary timestamps, one refreshed/one expired peer, status-only traffic,
member counts, pairs, and shared-IP greeting survivors.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink`, then
`RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.
