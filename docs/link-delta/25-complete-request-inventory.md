# 25. Inventory every Link request and reply before a full parity claim

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The existing audit lists inspected handlers and gaps but does not prove an exhaustive request/response inventory across connection modes, devices, and follow-up flows.

## Task

Build a versioned coverage matrix from vendor dispatchers, callbacks, captured packets, and client firmware. For each operation record request shape, state guard, response or silence, mutation, follow-up, RBX route, and evidence level.

## Completion evidence

A reviewer can trace every reachable vendor operation to an RBX behavior or an explicit, scoped exclusion, with no unexplained handler or callback left in the claimed surface.

## Sources and limits

V1–V7 vendor dispatchers; R1–R5 RBX modules; link-delta.md evidence appendix.

This is an evidence gate. Handler names and absent captures do not prove a command is unused.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

