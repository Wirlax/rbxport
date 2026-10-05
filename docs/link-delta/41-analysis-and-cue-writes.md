# 41. Implement or correctly refuse analysis and cue save requests

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

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

## Step 1 — investigation (2026-10-05)

Disposition: **per-operation guards/refusals mapped; durable write formats blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

The original fallback description is stale: R3 now sends 0x4003 for these
unsupported commands and preserves the menu (issue 01); it does not claim a
successful empty menu. [OBS] V4 `OnWriteCmd` (291–425) uses a shared-DB
lock and routes six distinct operations. Internal return 1 is not necessarily
a wire ACK. The inspected helpers establish:

| Request | Source-established behavior |
| --- | --- |
| 0x2005 | SavWave (4650–5008): non-shared DB, context byte 1 == 4, argument 5 length >= 900 and argument 6 blob. Argument 2 names the track; first 800 bytes become 400 loud-wave pairs and next 100 dot-wave bytes. Two store calls; no reply emitted here and no proven atomic rollback. |
| 0x2105 | SavUsbCue (4429–4650): argument 3 length >= 36, argument 4 aligned blob, first flags word <= 0x3ffff. Argument 2 selects delete vs save; argument 5/6 carry millisecond sidecar. Guard callback refusal yields empty cue response status 5. Successful route rereads cues; malformed initial record reports a local error, not a uniform scalar refusal. |
| 0x2205 | SavVbrInf (4173–4189) is a no-op returning 1 in this build. No storage call or reply here; do not invent a VBR writer from its name. |
| 0x2705 | SavUsbCueExt (5008–5532): length >= 56/aligned data, slot <= 8, type 1/2, timebase 75/150/1000. Initial size failure returns extended-cue status 0x32; invalid slot/type/timebase rereads existing cues; permission guard returns status 5. Successful mutation rereads extended cues, with optional comment/color/precise-time fields. |
| 0x2805 | SaveSpecifiedAtomInfo (4189–4327): extension DAT/EXT, atom PVB2/PQT2/PQTZ, reserved argument 4 zero. Length expression rejects 1..11 but visibly permits zero; PVB2 deliberately skips MstSaveAtomData. Scalar 0/0x32 after path/helper handling. |
| 0x2905 | UpdateSpecifiedAtomInfo (4059–4173): only EXT/PQT2, reserved argument 4 zero, length argument 5 >= 12; calls MstUpdateAtomData with argument 6 and blob argument 7. Scalar 0/0x32. |

All replies use the request transaction/connection route. Cue updates are
gated local event 1 through `DeliverCueUpdate` (4327–4348), not a proven
network broadcast. Extended-cue helper returns 1 even after some failure
responses, so notification behavior must be tested, not inferred from a
success-sounding return. Preserve non-target records and do not copy
unsafe pointer/size assumptions into Rust.

### Unknowns and bounded evidence attempt

Read OnWriteCmd and all six exported helpers, plus read-side cue/atom
formats and current fallback. Searched exports for analyzer store/update
and backend cue targets; their durable implementation is not included.
The inspected historical request list has no six-command write sequence.
[UNKNOWN] complete optional payload schemas, backend authorization, atom
store/update semantics, transaction/rollback, and final replies where only
internal returns appear. Evidence task: recover these callees and capture
write→read→reopen plus failures per variant. A universal 0x32 or ACK would
contradict the already distinct branches above.

### Implementation handoff

Affected: R3 typed write codecs/dispatch, R4 analysis/catalog invalidation,
`rbl-anlz`, `rbl-db` and app fixture-safe write boundary.
Dependencies: 13–15, 25, 39–40. Fixtures need each version and timebase,
memory/hot/loop cues, optional tails, malformed/reserved lengths, permission
failure, interrupted second storage operation and reopen hashes.
Smallest prerequisite: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.
Only individually proven refusal behavior may be added before storage
contracts close; no installed analysis file is a test target.
