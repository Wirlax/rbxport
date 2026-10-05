# 39. Match remaining database scalar, notice, and silent replies

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The report identifies reply differences for rating, history removal context, 3203/3503 notices, 3402 menu side effects, and other unsupported branches.

## Task

Audit the complete database dispatch/reply matrix, then correct one command's envelope, silence, result value, and menu-state effect at a time. Preserve device-specific additions with verified reply shapes.

## Completion evidence

Packet fixtures cover success, failure, foreign context, unsupported command, and follow-up render for each row; no false success or unsolicited reply remains in the claimed surface.

## Sources and limits

V4 OnDbModCmd/OnHistoryCmd/OnOtherCmd/OnUnknownClientCmd; R3 session.

Do not translate all backend booleans with one common convention; vendor commands use different scalar meanings.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

