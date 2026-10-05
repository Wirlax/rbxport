# 30. Handle incoming beat and master handover traffic

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] rekordbox receives master request 26, response 27, and beat 28 on UDP 50001; RBX has no receiver there.

## Task

Trace the message manager and client interpretation for 26/27/28, then implement the needed listener, state transitions, and replies, including arbitration and timeout behavior.

## Completion evidence

Captured master takeover/release and beat sequences produce the same externally observable state, packets, and failure handling on a supported device.

## Sources and limits

V4 startRecv; V5 ShortInterval; R2 beacon/beat/master logic.

Forwarding to a vendor callback is not proof of the final handover outcome; finish the callback trace first.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

