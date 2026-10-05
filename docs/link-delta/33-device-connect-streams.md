# 33. Support the DeviceConnect and DeviceConnect2 stream lifecycle

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor has separate framed stream reassembly handlers; RBX's UDP 16/17 handshake is a different path.

## Task

Trace both stream protocols through connect, message dispatch, disconnect, and error callbacks. Add an RBX path only for the supported client types identified by that trace.

## Completion evidence

A client fixture replays complete, split, repeated, and truncated frames and obtains the evidenced replies and state. Record whether desktop/mobile clients require each path.

## Sources and limits

V4 DeviceConnect; V5 DeviceConnect2; R2 UDP handshake.

A UDP handshake reply does not establish stream or mobile parity.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

