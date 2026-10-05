# 08. Expire silent peers without requiring more incoming traffic

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

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

