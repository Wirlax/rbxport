# 02. Decode and answer occupied-number bitmasks correctly

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX compares a number field where rekordbox reads subtype-2 bitmasks and echoes a counter.

Direct consequence and boundary: No number-collision rate or hardware failure has been measured.

## Do this one thing

Replace the assumed subtype-2 interpretation with the evidenced wired/wireless masks and counter handling. Preserve ordinary subtype-0 probing as a distinct format.

## Evidence and code

V1/V6 readIdBlkRequset; R2 NumberProbe/Join::hear_probe. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Use packet fixtures for set/unset bits, wired and wireless ranges, nonzero counters, truncated requests, and subtype-0 regression. Check exact reply bytes and whether a reply is sent.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **packet contract established; mode plumbing required**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `frameRead` (6670–6830) passes payload at frame offset
  `0x24` to `readIdBlkRequset` only for kind/subtype `02/02`.
  V1 `readIdBlkRequset` (7092–7149), also V6 at 1702, accepts vendor
  states 6 or 8. Wired mode `linkif=1` tests payload byte `0x0c`
  (frame `0x30`), bit `number-17`; wireless mode `linkif=0`
  tests payload `0x0f` (frame `0x33`), bit `number-41`.
  Counter is payload `0x1f` (frame `0x43`): decoding requires at
  least 68 bytes. This is not the 50-byte subtype-0 layout.
- On a matching bit, send a 39-byte `03/00` to the request's payload
  IPv4 address (frame `0x24..0x28`), announcement service port:
  header magic/name/version from the configured sender; length `00 27`
  at `0x22`; own number at `0x24`; request counter at `0x25`;
  status `01` at `0x26`. No membership mutation or follow-up request.
  Unmatched mask, wrong mode, or nonrunning state produces no reply.
- [OBS] V1 `readIdUseRequest` (7060–7090) also copies subtype-0
  number **and counter** from frame `0x2e/0x2f` to reply `0x24/0x25`.
  R2 `NumberProbe::decode` currently applies that 50-byte shape to
  subtype 2; `Join::hear_probe` compares one number; the reply helper
  hardcodes counter zero. Correct both variants without changing assignment
  subtype 1 into an occupied-number answer.

### Unknowns and bounded evidence attempt

Read V1's two reply constructors and V6's duplicate export; offsets and
counter copying agree. The five-capture announcement scan found no probes
or replies, so capture confirmation remains [UNKNOWN]. Full accepted
vendor length policy beyond the highest consumed byte is not proved by
this callback. Require safe truncation rejection in RBX; do not claim its
malformed-input policy matches the vendor. Issue 12 traces mode detection;
Mode23 and out-of-range bit shifts need separate evidence before extension.

### Implementation handoff

Affected: R2 `rbl-prolink/src/lib.rs`, `join.rs`, `beacon.rs` mode
configuration and tests. Dependency: explicit wired/wireless mode from 12
(or an already established wired-only scope); do not infer it from the
candidate number. Fixture matrix: both masks, set/unset bit, counters 0/1/255,
subtype-0 nonzero counter, truncation before every accessed field, unknown
subtype, waiting/probing/running, and echoed local IP+MAC. Store complete
datagrams plus exact destination and unchanged state.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink`, then
`RB_LITE_TEST=1 cargo test -p rbl-link join::tests`.
