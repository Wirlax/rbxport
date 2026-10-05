# 15. Answer key-information request 2a04

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox uses LoadKeyInf and returns 4c02; RBX falls back to an empty menu.

Direct consequence and boundary: Do not assume this binary payload is merely the human-readable key or the scalar key ID.

## Do this one thing

Trace the helper's actual data source, binary layout, context requirements, and failure path, then implement that read route.

## Evidence and code

V4 analysis dispatcher; V6 LoadKeyInf; R3/R4. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Check exact 4c02 replies for available/unavailable inputs and malformed requests; record client interpretation if it is used in a core load trace.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **segmented-key data source/layout established; golden validation pending**.
[Evidence key](investigation.md).

### Contract and current coverage

[OBS] V4 `OnSongAnlzCmd` (3429–3470) routes
`2a04 [context, content_id]` to V6 `LoadKeyInf` (365–475).
It resolves the track's analysis path and loads the same-stem `.EXT`,
using `MstLoadSegmentedKey` (3140–3265). This is PKEY segmented analysis,
not the scalar musical key or a display string.

The loader scans PMAI sections for PKEY, reads big-endian stride at PKEY+14
and count at +16, and reads records from the section header length. Each
output record is 12 bytes: convert the first and third 32-bit words from
big-endian to native little-endian; preserve the middle four bytes.
The reply blob is zero-initialized to `24 + 12*count`: count as u32 at 0,
u16 stride 12 at 4, u32 record-byte count at 8, zero at 12, records at 16,
and eight trailing zero bytes. Preserve those trailing bytes; do not shorten
the allocation to its apparent header+records size.

Success: same transaction/session `4c02 [2a04, 0, length, blob]`.
When no nonzero successful segment result is returned, the V4 caller sends
`4c02 [2a04, 0x32, 0, empty]` through `RetBinToClient`.
No state/menu mutation or write follows. R3 now returns issue 01's `4003`
for this opcode; the issue's old "empty menu" description is stale.

### Unknowns and bounded evidence attempt

Read V6's complete helper and loader plus V4's fallback. The checked-in
trimmed EXT has no PKEY, and searches of existing decoded sessions found
no `2a04/4c02`. [UNKNOWN] semantic meaning of the middle four bytes,
client consumption, and malformed-request acceptance. The loader also
handles concatenated PMAI blocks; don't infer ordinary section scanning
covers that case. Evidence task: retain PKEY-bearing EXT and full reference
reply, exercise zero-count and failed second load, then trace the actual
client. Partial-loader failure has a suspicious count/non-null interaction;
verify it before treating every corrupt EXT as the same wire failure.

### Implementation handoff

Affected: R3 command/analysis types and session route, R4 catalog analysis
path, `rbl-anlz` PKEY reader if shared. Dependencies: 01 and 23 for core
load claims. Fixtures: two unequal records, nondefault input stride,
zero-count, no PKEY, truncation/overflow, missing content, and full envelope.
Do not alter scalar key routes.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-anlz`, then
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.
