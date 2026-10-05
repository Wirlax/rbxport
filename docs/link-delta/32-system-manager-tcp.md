# 32. Implement the system-manager TCP request path

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor SysMgrTCP listens on TCP 50000 and forwards checked frames; RBX exposes database TCP services instead.

## Task

Trace the frame parser, callback operations, lifecycle, and actual client connection sequence; implement the evidenced service without conflating it with RemoteDB.

## Completion evidence

Recorded TCP frames produce equivalent replies, state changes, close/error behavior, and reconnect behavior. Confirm client need with a capture or firmware path.

## Sources and limits

V2 SysMgrMainComponent constructor; V5 SysMgrTCP; R1/R3 sockets.

No universal TCP operation set can be inferred from the existence of a listener.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

