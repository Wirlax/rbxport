# 27. Match the compatibility reset state guard

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The inspected vendor 0b reset runs only in nonzero state; RBX resets without the same guard.

## Task

Trace the vendor's state values and add the matching guard to RBX reset handling, including member, number, and timer changes for accepted resets.

## Completion evidence

A 0b packet in the vendor ignored state leaves RBX state unchanged; accepted-state reset reproduces evidenced outgoing/state behavior.

## Sources and limits

V2 readCompatibiltyMode; R2 hear_announce/Join.

A matching guard alone does not prove overall compatibility-mode support.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **static reset guard/state contract established; wire follow-up unverified**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] The actual exported function is V2 `readCompatiRes` (745–845).
It does nothing when runtime state byte `this+0x164` is zero. Otherwise
it removes active entries among 80 member slots, stops all timers, sets state
zero, notifies the local consumer, clears runtime/config data, restores
defaults (number 17, or 23 under the separate mode flag), sets network mode
0xff and reconstructs the announcement template. This routine has no
network send. Its guard means non-idle, not “has an assigned device number”:
a probing state can be nonzero before a final number exists.

[OBS] R2 join compatibility-reset handling clears/reset state without the
same idle guard. Keep the idle invocation entirely inert, including pending
state, notifications and timer scheduling; active cleanup must use the same
member/lifecycle ownership as other resets rather than only zeroing the
device number.

### Unknowns and bounded evidence attempt

Read the whole routine, V1 `frameRead` (6670–6832) and R2 reset handling.
`frameRead` routes kind 0x0b after the enabled, compatibility and wireless
original-model gates; it imposes no additional subtype/target check there.
The shared announcement scan found no compatibility-reset packets.
[UNKNOWN] receiver length policy, client follow-up for each compatibility
mode, and notification effects outside this routine. Evidence task:
capture an idle and a non-idle compatibility request and trace local
notifications through V3 before claiming full reset parity. Do not add an
ACK or automatic restart on the basis of this function.

### Implementation handoff

Affected: R2 join/reset tests and beacon lifecycle consumers. Dependencies:
09–11, 20, 26 and 29 for common teardown effects; do not use this issue to
invent their blocked behavior. Fixtures: idle, discovering, probing,
running, active multi-deck peers and repeat reset. Compare members, timers,
default mode/number and emitted actions; assert no wire reply from the
reset handler. Smallest check:
`RB_LITE_TEST=1 cargo test -p rbl-link --lib join`.
A source-backed idle guard can be isolated from unresolved downstream
cleanup work.

## Step 2 — implementation (2026-10-05)

Narrow implementation: an idle/absent Join ignores compatibility responses
before member/greeting reset. This matches the established idle guard and
prevents the existing broad teardown from erasing idle state.

A complete synthetic announcement regression verifies unchanged peer/player
maps, pending/completed greetings and Waiting state.
`RB_LITE_TEST=1 cargo test -p rbl-link --lib` passed.
Full compatibility processing remains blocked: outer compatibility flags,
all non-idle stop/default-mode transitions and downstream service/session
teardown retain the Step 1 evidence dependencies. No new reset sequence was
invented.
