# 36. Implement play-count and MyTag browse families

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor list dispatcher has play-count family 0e and MyTag family 15; RBX has no explicit menus for them.

## Task

Trace hierarchy, filters, sort, item layout, pagination, context, empty state, and follow-up requests; implement each family with the existing indexed query boundary.

## Completion evidence

A client can navigate both families to tracks, with vendor-equivalent envelopes/order for representative libraries and correct empty/unknown-item behavior.

## Sources and limits

V4 OnListClientCmd and called helpers; R3 menu dispatch; R4 catalog/index.

These families are deferred from basic parity; handler presence alone is not a complete browse implementation.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

