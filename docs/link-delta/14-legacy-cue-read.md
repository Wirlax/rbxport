# 14. Answer cue request 2104 with the expected cue envelope

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox collects/sorts cues and returns 4702; RBX has no 2104 handler.

Direct consequence and boundary: Depends on 01 for unsupported/error behavior, not on treating every cue format as interchangeable.

## Do this one thing

Implement the evidenced 2104 request context, cue record layout/order, reply counts, and unavailable/error behavior. Keep 2504 VBR and 2b04 extended cues separate.

## Evidence and code

V4 analysis dispatcher; V6 GetUsbCue; R3/R4. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)
- [crates/rbl-link/src/blobs.rs](../../crates/rbl-link/src/blobs.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Use no-cue, memory-cue, hot-cue, and loop fixtures with captured/reference-proven bytes; check counts/order and follow-up load behavior. Preserve existing extended-cue tests.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **legacy layout established; golden fixtures and error boundary pending**.
[Evidence key](investigation.md); RX3 client source is linked there as F1.

### Contract and current coverage

[OBS] V4 `OnSongAnlzCmd` routes `2104 [context, content_id]` to V6
`GetUsbCue` (65–362), separate from `2504` and `2b04`. The helper
reads djmdCue by content ID and sorts column 10 (cue kind), or sorts callback
cues with CueKindComparator. It skips kinds >4, packs memory kinds 0/4
before hot kinds 1–3, and limits total processed records to 13.

Each little-endian record is 36 bytes. Its word at 0 is
`flags | slot<<16`: presence `0x100`, loop bit 0, and `0x200`
for a kind-4 loop. Bytes 4–11 remain zero. In/out frame words occupy 12/16;
source auxiliary words occupy 20/24/28/32. V6
`CmnFunc_MsecToFrm150_InOoutConv` (2115–2141) uses truncated
`ms*0.15`; loop out is decremented once if nonzero, nonloop out-frame is
0. Nonloop sidecar end equals its start, not `u32::MAX`. Sidecars are
8 bytes per entry, native little-endian in/out milliseconds.

V6 `RetCueToClient` (2357–2433) establishes `4702` with original
transaction/request kind, status, `36*n`, records, stride 36, hot count,
memory count, `8*n`, sidecars. RX3 F1 `dbcl_WaitCue` at `0026194c`
also consumes the trailing data-length/pointer fields; the RBX eleven-field
envelope includes their zero/empty values. Zero cues yield status 1 in the
vendor helper. Delivery is on the requesting DB session; no menu replacement
or persistence.

R3 has an existing `usb_cue_reply` (495–543) for a hot-bank edit follow-up,
but no `2104` dispatch. It uses `u32::MAX` for absent out-time and
does not subtract the loop-end frame. It cannot simply be reused as proof
of the desktop contract. R4 already exposes `Catalog::usb_cues`.

### Unknowns and bounded evidence attempt

Read V6's full construction, rounding helper and reply, R3's existing helper,
and RX3 F1 parser. Existing decoded captures contain no `2104/4702`
pair; the existing `4502` fixture belongs to VBR. [UNKNOWN] complete
vendor serialization across setup variants, malformed-context behavior,
and equal-kind tie ordering. Evidence task: export serializer/tie comparator
or capture no-cue, memory, A–C hot-cue, and loop cases in both setup modes.
These portions remain blocked; do not repurpose an extended-cue blob.

### Implementation handoff

Affected: R3 constants/`session.rs`, R4 cue adapter/`blobs.rs`, focused
session/blob tests. Dependency: 01; coordinate shared helper changes with
hot-bank regressions. Use temporary cue fixtures including mixed ordering,
kind >4, 13-entry boundary, loop rounding and absent data. Assert full bytes,
counts, next load request, and unchanged active menu.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`,
then `RB_LITE_TEST=1 cargo test -p rbl-link --test blobs`.
