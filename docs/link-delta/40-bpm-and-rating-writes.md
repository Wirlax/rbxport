# 40. Persist BPM and rating changes with update delivery

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

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

## Step 1 — investigation (2026-10-05)

Disposition: **dispatch/reply/update ordering established; backend validation contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `OnDbModCmd` (425–507) requires shared-DB lock/open. 0x2507
passes DB index, full context, track argument 1 and argument 2 to backend
+0x2a8; its log names the value NewBPMx100 and a cloud flag, but the packed
argument's complete interpretation is not established. 0x2107 passes DB
index/context/track, only argument 2's low eight bits, and an output byte
to +0x2a0. Missing callback yields -2. Each replies through Ret4ByteToClient
with the backend result unchanged: success 1, not 0.

Only result == 1 invokes `DeliverBpmUpdate` or `DeliverRatingUpdate`
before queuing the scalar response. Helpers (5532–5583) require notification
flag bit 1 and a callback, then post local event 6 or 5 with the track ID.
They are not themselves network update packets; downstream delivery still
needs tracing. Lock-open failure returns internal -2 before the reply path.

[OBS] R3 rating validates 0..5, edits via Catalog and responds 0 on success/
1 on failure; 0x2507 has no handler. R4 forwards edits to Source.
`src-tauri/src/link.rs::AppSource::edit` (194–239) already writes rating
through `write_then`, refreshes touched metadata and emits its app event;
no BPM Edit variant exists. Existing rating persistence is not missing,
but its scalar and vendor-specific update contract differ.

### Unknowns and bounded evidence attempt

Read dispatch, update helpers, R3/R4/app edit path and rating tests.
The inspected historical request list has no 0x2107/0x2507 transaction;
rating screenshots/test names alone do not establish reply bytes.
[UNKNOWN] BPM argument packing/ranges, cloud/foreign-context validation,
negative backend codes, rollback after refresh failure and network effects
of local events. Evidence task: export both backend targets and update
consumer, then capture bounded writes/rereads on a temporary fixture.
Do not reproduce low-byte truncation as authorization to accept arbitrary
input without documenting the safety boundary.

### Implementation handoff

Affected: R3 constants/Edit/result/session, R4 Source, app edit adapter
and `rbl-db` Writer/index refresh. Dependencies: 25, 39, 41 for analysis
side effects if the backend establishes any. Fixtures: valid/invalid
ratings, high bits, BPM boundaries, unknown track, foreign context,
read-only catalog, storage/refresh failure and reopened DB; assert exactly
one evidenced event and the full response. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session rating`.
All future write checks require `rbl_db::fixture::build`.
