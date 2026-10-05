# 42. Match account and user-info response behavior

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor 3006 is callback-backed and can return data or empty; RBX always returns 160 zero bytes.

## Task

Trace the vendor callback, field meanings, availability guard, privacy boundary, and client follow-up before implementing any populated response.

## Completion evidence

For available/unavailable cases, full 4d02 envelopes and client follow-ups match the declared scope without fabricating identity fields or leaking host account data.

## Sources and limits

V4 OnUserCmd; R3 USER_INFO; R4 blob helper.

The existing zero reply may satisfy a particular load flow; that does not establish parity for account features.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

