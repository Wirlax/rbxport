# 16. Return browse type for the requested context

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox returns 1 for context byte 4 and 0 otherwise; RBX always returns 1.

Direct consequence and boundary: Client branch selection from the value still needs decoding/capture evidence; no specific screen failure is assumed.

## Do this one thing

Implement the evidenced context-dependent 3303 scalar reply while preserving the device-compatible supported query.

## Evidence and code

V4 OnOtherCmd; R3 BROWSE_TYPE. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Assert complete replies for context 4, other contexts, and malformed input; test the supported device browse path remains available.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **desktop scalar contract established; RX3 compatibility evidence required**.
[Evidence key](investigation.md), including RX3 F1/F2.

### Contract and current coverage

[OBS] V4 `OnOtherCmd` (848–859) reads byte `command+0x19`,
the second little-endian byte of argument 0: `(context >> 8) & 0xff`.
This is distinct from R3's menu location `context.to_be_bytes()[1]`.
For `3303`, return 1 only when that context byte is 4, else 0.
V4 `Ret4ByteToClient` (1686–1743) defines same transaction/session
`4000 [3303, value]`. The foreign-context branch also emits a local error
notification, not an additional network `4003`. No menu replacement,
library mutation or follow-up packet is established.

R3 `BROWSE_TYPE` currently always returns 1. F1
`dbcl_GetBrowseType` (`00262fa0`, 2301–2359) checks device-property
browse kind first, then sends `3303` only as fallback and interprets
the scalar using `DBCommon_GetBrwsKind`. F2's device **server**
`DBSMain_OnOtherClientCmd` (3005–3054) uses a different slot/backend
mapping. Do not mistake that device server's contract for desktop rekordbox.

### Unknowns and bounded evidence attempt

Read V4's scalar branch/reply, R3, and both RX3 paths. Existing captured
desktop sessions searched here contain no `3303` query. [UNKNOWN]
which fallback context the real RX3 uses against RBX's advertised properties,
and whether changing its present reply alters supported browsing.
Evidence task: inspect request SetHeader/property mapping or record RX3
fallback against the fixture before replacing its currently supported
behavior. Missing/wrong-typed argument policy is not proven by a decoded
vendor callback; fail safely without claiming malformed-wire parity.

### Implementation handoff

Affected: `rbl-dbserver/src/session.rs`, session/codec fixtures; keep a
small explicit context-byte helper rather than confusing it with menu
location. Dependencies: 01 and RX3 fallback evidence; 22 consumes this gate.
Fixtures: byte 4/3/0/255, independently varying player/location/type bytes,
both setup modes, absent/wrong argument, menu preserved, and supported RX3
browse sequence. Exact scalar tests alone do not discharge the RX3 gate.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.
