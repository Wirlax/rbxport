# 04. Reacquire a number when a joined peer claims ours

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox disconnects, clears membership, and retries after a running-state collision; RBX has no equivalent transition.

Direct consequence and boundary: Depends on 02–03. Do not invent the vendor random-delay distribution from its name.

## Do this one thing

Implement the evidenced running-state collision transition, including disconnect output and reset/retry state. Recover timer/state details before choosing a retry policy.

## Evidence and code

V1 readConfigNotify; R2 Join/beacon. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

With deterministic time/randomness, assert disconnect output, reset, and negotiation of an unoccupied number; verify unrelated keepalives and looped-back self packets do not trigger it.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **implementation blocked on retry-timer evidence**.
[Evidence key and capture check](investigation.md).

### Established contract and current coverage

- [OBS] V1 `readConfigNotify` (7316–7402) checks a valid keepalive in
  states 6/8, after the CDJ/XDJ coexistence guard. Matching own number
  triggers collision handling. R2 `Join::hear_keep_alive` has no running
  collision branch; `hear_announce` already excludes exact local IP+MAC.
- Vendor state 8 stops its `this+0x6d0` timer. Each active member
  becomes removed, decrements the member count, clears the type-2 marker
  where applicable, and emits local notification 4.
- Then broadcast `08/00`, length 41: own number at frame `0x24`,
  own IPv4 at `0x25..0x29`, configured common header. This is a broadcast
  announcement, not a unicast reply to the colliding peer.
  Remove local membership accounting, enter vendor state 3, notify the
  local state change, restore the configured candidate `this+0x14b`,
  and start the retry timer at `this+0x6d8`.
- Do not insert the colliding peer as a normal member afterward, continue
  advertising the old number, or restart immediately in `Waiting`
  and call that the vendor transition.

### Unknowns and bounded evidence attempt

Read the complete collision path and searched V1/V2 for `_rand`,
`0x15a`, `timerFuncRandom`, and `timerFuncIdRetry`. V2 timer
dispatch names the follow-up callbacks; the inspected exports do not define
those callbacks. The delay decompiles as
`int(float(rand()) * -4.656613e-10 * float(*(u16*)(this+0x15a)+1))`.
The negative scale is not a defensible random-delay distribution.
The five existing captures contain no collision/reset packets.

[UNKNOWN] actual arithmetic, timer clamping, and subsequent acquisition
transition. Evidence task: export/disassemble the collision timer call and
`timerFuncRandom/timerFuncIdRetry` from this exact build, or collect an
authorized deterministic collision trace. Implementation remains blocked;
do not substitute an arbitrary jitter constant.

### Implementation handoff

Affected: `rbl-link/src/join.rs`, `beacon.rs`, packet constructor in
`rbl-prolink`; session readiness interacts with 11. Dependencies: 02–03,
and resolved timer evidence. Fixtures need separate local/remote MACs,
same own number, unrelated numbers, coexistence modes, deterministic
time/randomness, exact disconnect bytes/destination, full membership clear,
and eventual free-number selection. Test self-echo and nonrunning silence.

Smallest validation after evidence resolution:
`RB_LITE_TEST=1 cargo test -p rbl-link join::tests`, then the beacon
socket regression. No device parity is claimed now.
