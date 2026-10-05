# 40. Persist BPM and rating changes with update delivery

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor 2507 performs BPM edit/update; 2107 returns backend result 1 and delivers rating update. RBX lacks 2507 and uses a different rating success scalar.

## Task

Trace arguments, validation, persistence, secondary update messages, and error codes. Implement BPM edit and align rating wire reply while preserving safe database writes.

## Completion evidence

Fixture writes update the intended track, persist, emit exactly the evidenced notification, and return correct success/failure values; malformed or foreign requests cannot write user data.

## Sources and limits

V4 OnDbModCmd; R3 CHANGE_RATING; R4 source edits.

Use RB_LITE_TEST=1 and temporary fixture libraries. Never exercise writes against the installed rekordbox library.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

