# 43. Match portmapper and mount procedure behavior

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor portmapper checks service version and accepts SET/UNSET only from loopback; its mount dispatcher handles EXPORTALL but not DUMP. RBX differs on these procedures.

## Task

Implement version-aware GETPORT, loopback SET/UNSET results, and the observed mount procedure set only where required for the claimed behavior; verify map lifetime and reply envelopes.

## Completion evidence

Program/version/transport combinations, local versus remote registration, DUMP, EXPORTALL, MNT, UMNT, and unknown procedures have verified replies and side effects.

## Sources and limits

V7 portmapper/mount dispatch; R5 portmap/mount.

A broad host export is not proven and must not be introduced to imitate an untraced UI decision.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

