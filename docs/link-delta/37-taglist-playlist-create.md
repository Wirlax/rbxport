# 37. Create a playlist from the Tag List on request 3102

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor OnPrepareCmd calls backend tag-list/playlist operations, sends a playlist update, and replies 0 or ffffffff; RBX has no handler.

## Task

Recover the exact source/target arguments, playlist naming/order, transaction, failure/rollback, and update callback. Add a guarded write with durable persistence.

## Completion evidence

A fixture request creates exactly the expected playlist/order; duplicates, empty tags, invalid context, and storage failure match reply/rollback/notification behavior.

## Sources and limits

V4 OnPrepareCmd; R3 dispatch; R4 source/edit methods.

Do not reinterpret 3102 as tag-list reordering or acknowledge creation before a durable write.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

