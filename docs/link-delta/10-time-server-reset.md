# 10. Honor targeted time-server reset requests

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox handles 04 in specified running states by resetting members and number acquisition; RBX ignores it.

Direct consequence and boundary: This is a specific lifecycle request, not an instruction to implement every clock/sync feature.

## Do this one thing

Add the targeted request branch with the evidenced state guards and reset transition.

## Evidence and code

V2 readTimeServerRequest; R2 announcement dispatch. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test matching/nonmatching target numbers, running/nonrunning states, membership clearing, and restart of acquisition. Assert exact outgoing packets only when established.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **reset branch established; implementation blocked by retry details**.
[Evidence key and capture check](investigation.md).

### Established contract and current coverage

- [OBS] V1 `frameRead` dispatches kind `04` with payload at
  `0x24`; V2 `readTimeServerRequest` (483–575, `100f4ba18`)
  requires vendor state 6/8 and payload byte 0 equal to the local number.
  Nonmatching target and other states have no mutation or reply.
- For a match, stop the state-8 timer, remove/notify active members,
  remove local membership accounting, change to state 3, notify the state,
  restore the configured candidate, and start timer `this+0x6d8`.
  **This function does not broadcast `08`**, unlike issue 04.
  No immediate wire response is present; later acquisition is timer-driven.
- R2 `hear_announce` drops `04`; `Join::reset` goes directly to
  waiting, which is not proof of this intermediate retry state.

### Unknowns and bounded evidence attempt

Read V2's whole function and compared V1's collision body. Both use the
same suspicious negative random-scale decompilation and missing retry
callbacks documented in 04. The shared capture check contains no `04`.
[UNKNOWN] actual retry delay, clamp, and next timer transition. Evidence
task: resolve the exact retry arithmetic/callback in 04 and add a targeted
`04` trace. Do not add a disconnect packet because another reset path
uses one, or invent a fixed delay.

### Implementation handoff

Affected: `rbl-prolink` targeted request decoding; `join.rs` and
`beacon.rs` lifecycle handling. Dependencies: 04's resolved retry state
and 11's session identity behavior. Fixtures: matching/nonmatching number,
states 6/8 versus nonrunning equivalents, complete membership state,
malformed/truncated target, no immediate outbound packet, and deterministic
later acquisition after evidence resolves the timer.

Smallest validation after evidence resolution:
`RB_LITE_TEST=1 cargo test -p rbl-link join::tests`, followed by the
beacon socket test.
