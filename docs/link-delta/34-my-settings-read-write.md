# 34. Serve My Settings reads and writes

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor NormalInterval recognizes MySetting read/write, DJM MySetting read/write, and device-setting read/write; RBX has no equivalent dispatch.

## Task

Trace each request to its callback, data schema, authorization/context, storage, success/failure reply, and notification. Implement only validated fields and refuse unsupported writes with the correct envelope.

## Completion evidence

Controlled settings survive the evidenced read/write/reconnect sequence; invalid fields and unsupported versions receive client-consumable errors. Confirm settings UI behavior on the claimed device.

## Sources and limits

V5 NormalInterval/matchers; R2 status dispatch.

Do not store opaque or guessed settings bytes as a claimed successful write.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

