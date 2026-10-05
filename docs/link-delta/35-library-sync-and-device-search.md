# 35. Handle device search, disconnect, and library sync messages

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor receiver matches DeviceSearch2, DeviceDisconnectReq/Res, GetLibrarySyncStatusRes, GetName, SyncStatus, SDP Link/HID, and related responses; RBX drops them.

## Task

Trace the reachable operations and their callbacks, then implement the request/response and state paths required by the declared full-parity scope.

## Completion evidence

Every inventoried message has an intentional RBX handler or a documented exclusion; captured search, sync, and disconnect scenarios match vendor packet and state behavior.

## Sources and limits

V5 NormalInterval and match helpers; R2 status dispatch.

Some matched packets may be responses to features RBX never initiates; determine reachability before writing handlers.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

