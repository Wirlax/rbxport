# 37. Create a playlist from the Tag List on request 3102

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor OnPrepareCmd calls backend tag-list/playlist operations, sends a playlist update, and replies 0 or ffffffff; RBX has no handler.

## Task

Recover the exact source/target arguments, playlist naming/order, transaction, failure/rollback, and update callback. Add a guarded write with durable persistence.

## Completion evidence

A fixture request creates exactly the expected playlist/order; duplicates, empty tags, invalid context, and storage failure match reply/rollback/notification behavior.

## Sources and limits

V4 OnPrepareCmd; R3 dispatch; R4 source/edit methods.

Do not reinterpret 3102 as tag-list reordering or acknowledge creation before a durable write.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **callback sequence/reply established; persistent playlist contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `OnPrepareCmd` (622–731), command 0x3102, takes a shared-DB
lock and initializes the output playlist ID to zero. With a backend, it
calls +0x2f0 with DB index, argument 2 and an output ID; on positive result
it calls +0x2f8 with DB index, context, that ID and the low byte of argument
4. If the second result is < 1, it invokes +0x300 cleanup when the backend
still exists. The argument meanings and transaction guarantees are not
recoverable merely from those offsets.

After that sequence it calls `DeliverPlaylistUpdate` with the output ID
even on the failed path, then sends 0x4000 [0x3102, 0] for positive result
or [0x3102, 0xffffffff] otherwise, preserving the transaction.
`DeliverPlaylistUpdate` (5603–5623) is a gated local callback
(event 4, ID, 0), not itself a UDP/TCP broadcast. Lock-open failure exits
before that sequence with internal -2; its outer error path must be retained.

[OBS] R3 has Tag List add/clear but no 0x3102 handler; it now reaches the
0x4003 unsupported fallback, not an empty menu. R4/app Source edit supports
other durable edits, not this playlist-creation sequence.

### Unknowns and bounded evidence attempt

Read the complete dispatcher/notification helper and current edit boundary;
searched exports and the historical interaction request list.
[UNKNOWN] argument-2 string/ID meaning, naming, parent, ordering, empty tags,
duplicate policy, callback result codes, actual storage rollback and
notification delivery beyond the local event. Evidence task: resolve
+0x2f0/+0x2f8/+0x300, then trace isolated create/failure/duplicate cases with
before/after database and full replies. No 0x3102 capture was found in the
inspected request list; that is not proof of non-use.

### Implementation handoff

Affected: R3 command and typed Edit/result; R4 catalog/Source;
`src-tauri/src/link.rs` and `rbl-db` fixture-safe Writer transaction.
Dependencies: 25, 39, existing indexed playlist-order semantics.
Fixtures: empty and ordered Tag Lists, duplicate tracks/name, target/foreign
context, callback unavailable, first/second phase failure and reopen.
Assert cleanup and event ID/order as well as reply; do not acknowledge before
the established durable operation. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`;
then focused `rbl-db` fixture tests once callbacks are established.
