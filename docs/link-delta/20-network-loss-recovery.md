# 20. Reset Link when its selected network actually disconnects

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] Vendor monitor checks network disconnect/IP/BSSID; RBX only checks interface name/address presence.

Direct consequence and boundary: No permission to widen binding to all interfaces or automatically connect on a different interface is implied.

## Do this one thing

Trace vendor transitions, then add evidence-backed detection/recovery for the selected interface when name/address alone remain present. Keep wireless detection platform-scoped rather than guessing portable APIs.

## Evidence and code

V2 timerCallback; R1 app reporter; R2 monitor. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [src-tauri/src/link.rs](../../src-tauri/src/link.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test a retained-address interface that becomes disconnected, IP change, and restart. Record actual wired unplug/replug and applicable wireless change behavior; report supported OS/device boundaries.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

