# 11. Do not serve sessions using an unnegotiated fallback number

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

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

