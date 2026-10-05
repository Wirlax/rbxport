# 27. Match the compatibility reset state guard

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The inspected vendor 0b reset runs only in nonzero state; RBX resets without the same guard.

## Task

Trace the vendor's state values and add the matching guard to RBX reset handling, including member, number, and timer changes for accepted resets.

## Completion evidence

A 0b packet in the vendor ignored state leaves RBX state unchanged; accepted-state reset reproduces evidenced outgoing/state behavior.

## Sources and limits

V2 readCompatibiltyMode; R2 hear_announce/Join.

A matching guard alone does not prove overall compatibility-mode support.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

