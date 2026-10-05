# 41. Implement or correctly refuse analysis and cue save requests

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor routes 2005/2105/2205/2705/2805/2905 to wave, cue, VBR, extended-cue, and atom saves; RBX currently falls back to an empty menu.

## Task

For each request, trace the payload version, target file/record, validation, atomic write, update delivery, and error reply. Implement proven formats; for unresolved formats send the evidenced refusal rather than false success.

## Completion evidence

Complete binary fixtures test success/failure, reopen persistence, notification, invalid size/version, and rollback for each implemented write. Unimplemented variants receive a consumable failure.

## Sources and limits

V4 OnWriteCmd; V6 related read helpers; R3 dispatch; R4 analysis/catalog.

Do not infer storage format from Sav* function names. Write tests require RB_LITE_TEST=1 fixtures.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

