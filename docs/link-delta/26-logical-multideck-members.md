# 26. Synthesize and remove the vendor's paired logical members

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The inspected vendor keepalive branch adds paired IDs for 9/11 and additional OPUS-QUAD entries; RBX waits for separate received IDs.

## Task

Map the exact model/number guards, add the paired members only under those guards, and keep disconnect, address update, ageing, and greeting state coherent across the group.

## Completion evidence

One received qualifying announcement creates the evidenced logical member set; disconnect and ageing remove the evidenced members without affecting unrelated decks. Capture device-visible membership where available.

## Sources and limits

V1 readConfigNotify; V2 readDisconnect; R2 beacon receive and membership.

The report does not establish that every multi-deck device requires synthesis.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

