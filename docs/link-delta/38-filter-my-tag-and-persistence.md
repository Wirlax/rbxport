# 38. Match filter conditions and session persistence

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor filter dispatcher has MyTag conditions and a context-associated setting manager with save-file support; RBX filter state is session-local and lacks MyTag operations.

## Task

Trace all condition encodings, owner/context lookup, save/reload triggers, and failure replies, then implement the evidenced scope.

## Completion evidence

A filter configured in one session returns the expected matches and persists or resets across reconnect exactly when the vendor does; MyTag conditions and 0x32 failure cases are covered.

## Sources and limits

V4 OnFilterCmd/filter helpers; V6 saveFile; R3 filter/session.

The existence of saveFile does not establish its call timing; recover it before selecting persistence behavior.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

