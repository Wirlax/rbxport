# 29. Reconcile mounted-host state with device disconnect and stop

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] RBX beacon disconnect/reset leaves NFS mounted-host state alive. Vendor has mounted-host structures and UMNT cleanup, but the exact device-disconnect path is unresolved.

## Task

Trace vendor UMNT, device-disconnect, restart, and stop paths, then make RBX mount identity and cleanup match the observed event boundaries.

## Completion evidence

For each traced event, test whether an old mount handle remains valid, whether a reconnect can remount, and whether one deck's disconnect affects another's mount.

## Sources and limits

V7 mount/UMNT helpers; R1/R2/R5 service lifetime.

Do not clear all mounts on every packet merely to remove stale state.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

