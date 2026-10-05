# 11. Do not serve sessions using an unnegotiated fallback number

Priority: P0. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX database connections can open before acquisition and advertise fallback 17; vendor database initialization is linked to its connection callback.

Direct consequence and boundary: Depends on lifecycle fixes. Do not merely reorder socket binds: the externally observable ready condition is the target.

## Do this one thing

Trace the vendor ready boundary, then make database session acceptance/identity coherent with the assigned Link number. Define how in-flight sessions behave during reacquisition from evidence.

## Evidence and code

V3 notifyLinkConnect; R1 startup; R3 session open/networking. Primary artifacts: [V3](../../../rbxport-private/verification/link/rekordbox-re/rb_bystring.c).

Implementation locations:

- [crates/rbl-link/src/lib.rs](../../crates/rbl-link/src/lib.rs)
- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-dbserver/src/net.rs](../../crates/rbl-dbserver/src/net.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Attempt sessions before join, after join, and during collision/reset. Verify no session invents number 17 and that valid joined sessions still work. Capture the vendor pre-ready behavior before specifying its exact refusal.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **direct-session gap confirmed; exact refusal/reacquisition blocked**.
[Evidence key and capture check](investigation.md).

### Established contract and current coverage

- [OBS] V3 `UiProDJLink::notifyLinkConnect` (4236–4296) calls
  `PSvDBServer::Initialize` only when not initialized, with the assigned
  number from the callback. This establishes the identity used at initial
  initialization, not a complete TCP refusal policy.
- R1 `Link::start` binds services and shares the beacon's number cell.
  R3 `Bound::start` (218–241) **already gates port queries** through
  `Handler::serving`. A pre-ready query closes unanswered.
  The DB accept loop directly calls `serve_session`, which calls
  `handler.open` without that gate. `CatalogHandler::open` (50–55)
  converts number zero to 17. This direct-session bypass remains a real gap.
- Established RBX invariant: a network session must never receive an
  invented identity while number acquisition is incomplete. Preserve the
  assigned number after readiness. Gate/check the direct session path and
  make opening plus identity selection coherent; a separate boolean check
  followed by a racy fallback-to-17 load is insufficient.

### Unknowns and bounded evidence attempt

Read V3 initialization and V1 `notifyLinkDisconnect` (11181 onward),
plus both RBX accept loops and session creation. The disconnect callback
posts a UI message; it does not itself prove accepted-session teardown.
The shared captures do not cover pre-ready connection attempts or
reacquisition. [UNKNOWN] vendor SYN refusal vs accepted close, daemon
lifetime after link-down, and handling of established sessions when the
assigned number changes. Evidence task: trace DB initialization/finalization
from the posted link events, then capture pre-ready, joined, and reset
connections. Exact transport refusal and in-flight-session policy remain
blocked; do not infer them from bind order.

### Implementation handoff

Affected: `rbl-dbserver/src/net.rs` handler/session-open boundary,
`session.rs`, `rbl-link/src/lib.rs`, and socket tests. Dependencies:
04/09/10/20 lifecycle transitions, without requiring their unknown behavior
to be fabricated. Fixtures use controllable number/readiness epochs,
direct DB-port and discovery-port connections, zero→17/18, reset races,
already-open sessions, and subsequent valid setup replies. No real library
writes are required.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-dbserver`; then
`RB_LITE_TEST=1 cargo test -p rbl-link --test beacon` plus the relevant
existing Link socket test once the lifecycle policy is evidenced.

## Step 2 — implementation (2026-10-05)

Narrow implementation: network session creation calls `Handler::open_ready`.
`CatalogHandler` overrides it with one atomic assigned-number snapshot:
zero returns no session; nonzero creates a session with that exact identity.
The direct open path no longer substitutes 17 for zero. An unready network
connection closes before any greeting/setup response, matching the existing
RBX query-gate safety policy.

Controlled snapshots cover 0→17→18→0 and retained identities of already-open
sessions. Real direct TCP-port fixtures check unanswered pre-ready/reset
connections, exact greeting/setup bytes with 17 and 18, and successful later
reconnection. `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`
passed **46 tests**.
Unanswered close is an RBX safety choice, not vendor refusal-timing proof.
Already-open sessions retain their current snapshot; vendor teardown/
reacquisition policy and the full lifecycle issue remain blocked.
