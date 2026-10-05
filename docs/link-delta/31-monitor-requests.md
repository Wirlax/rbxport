# 31. Handle Link monitor and reset messages

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] rekordbox receives 1f/20 on UDP 50004; RBX has no monitor receiver.

## Task

Trace the vendor callback and packet layouts, then add only the evidenced monitor/reset behavior and response timing.

## Completion evidence

For valid, invalid, and wrong-state monitor packets, compare exact responses, state changes, and timeouts with the reference; preserve basic discovery.

## Sources and limits

V4 startRecv; V5 Monitor; R2 beacon startup.

A listening socket alone is insufficient for parity.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

