# 07. Clear obsolete membership when a device rediscovers

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox removes matching stored MAC membership on discovery 00; RBX ignores it.

Direct consequence and boundary: Do not infer a universal MAC-only identity policy from this one branch.

## Do this one thing

Handle rediscovery using the evidenced MAC matching and state guards, clearing only the records that the vendor branch clears.

## Evidence and code

V1 readDiscovery/frameRead; R2 hear_announce. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Register identities, inject matching and unrelated discoveries, and assert member changes followed by successful fresh registration. Include shared-IP devices and malformed packets.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

