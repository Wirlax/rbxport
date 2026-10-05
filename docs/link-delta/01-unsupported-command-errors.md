# 01. Return an error without destroying the active menu

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] An unknown command produces an empty 4000 menu and replaces RBX's stored menu. rekordbox returns 4003 instead.

Direct consequence and boundary: The client's screen reaction is unknown; the menu replacement and reply difference are source-proven.

## Do this one thing

Implement the evidenced 4003 envelope for unsupported commands, preserving transaction/request kind and the active menu. Keep intentional silent commands and device-specific supported extensions explicit rather than routing them through a blanket error.

## Evidence and code

V4 OnUnknownClientCmd; R3 fallback/handle_menu. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Send an unsupported command between a menu request and its render: check the complete error envelope and that the previous menu still renders. Cover both setup shapes, different menu locations, and supported mobile-query behavior.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **existing core fix; validation contract established**. Baseline,
evidence keys, and capture limits are in [the investigation record](investigation.md).

### Contract and current coverage

- [OBS] V4 `PSvDBMain::OnUnknownClientCmd` at `101d91ed8`
  (224–277) preserves the transaction, sets kind `4003`, puts the original
  request kind in its first argument, and sends through the originating DB
  client route. It does not enter list construction or replace a stored menu.
  R3 `LinkSession::handle_menu` (961–969) now returns exactly
  `Message(tx, 0x4003, [Number(request_kind)])`. The old finding is stale.
- Accept a decoded command only after the session dispatcher has excluded
  supported commands and intentional silence. R3 `handle` keeps
  `TEARDOWN`/`SET_ON_AIR` silent and the explicit mobile query route
  separate. The unsupported response goes back on the same TCP session;
  preserve all menu locations, filter state, and setup-version state.
- [OBS] R3 `handle` distinguishes one-argument setup from a two-argument
  setup; C1 records `[5]` → `4000 [0,17]` and `[5,20]` →
  `0000 [17,20]`, followed by 12/16-argument render items. Those are
  fixtures for this regression, not proof about arbitrary version values.

### Unknowns and bounded evidence attempt

Read V4's complete handler, current fallback and dispatch, C1, and
`tests/session.rs::unsupported_command_keeps_the_active_menu_and_echoes_its_kind_in_4003`.
Ran that test: **1 passed**. It checks decoded messages and a retained root
menu; it does not cover both setup modes, independent locations, full
serialized bytes, or mobile-query regressions. [UNKNOWN] player-screen
interpretation of `4003`; C1 contains no unknown-command probe. A new
vendor/client unsupported-command trace is the evidence task for that claim.
Do not block the source-proven error/menu fix on a speculative UI symptom,
and do not declare the broader acceptance criteria met by this one test.

### Implementation handoff

Affected: R3 `session.rs`, `tests/session.rs`, and wire fixture tests in
`tests/messages.rs`. No new core fallback change is needed. Dependencies:
none. Add byte-exact request/error fixtures around an existing menu and
render for both setup shapes and at least two menu locations. Retain explicit
silent and mobile commands; malformed wire messages belong to codec/socket
validation and must not be relabeled successful requests.

Smallest validation:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session unsupported_command`,
then the new encoded-message regression. No physical-device parity is
established by this investigation.
