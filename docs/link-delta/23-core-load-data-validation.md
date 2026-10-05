# 23. Prove the basic track-load data sequence

Priority: P1 evidence gate. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] Waveform/grid/extended-cue/tagged-analysis routes exist; full load payload parity is unknown.

Direct consequence and boundary: Depends on 13–15. Do not fabricate account data or assume all 160 user-info bytes must match for basic desktop loading.

## Do this one thing

Compare one complete device-requested load sequence on representative controlled tracks, including existing user-info and delivery-info routes. Fix demonstrated payload/count/unavailable differences.

## Evidence and code

V4/V6 analysis and song-info helpers; R3 analysis/user-info/delivery dispatch; R4 blobs/catalog. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)
- [crates/rbl-link/src/blobs.rs](../../crates/rbl-link/src/blobs.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Cover waveform/grid, extended cues with comments/loops/colors, analysis tags, artwork, and absent data for the actual client sequence. Distinguish vendor callback account data from fields genuinely required by the client.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **load comparison contract defined; acceptance blocked by 13–15**.
[Evidence key](investigation.md).

### Required comparison and current coverage

Compare the actual client's requests, preserving transaction, context,
length/status/count fields and TCP session across metadata/path, waveform,
beat grid, legacy/extended cues, named analysis tags, artwork, user-info and
delivery-info. Assert exact data bytes plus subsequent client requests and
status naming the loaded content. File delivery must reconstruct the fixture
audio through NFS; an ACK alone is not load completion (18).

[OBS] R3 `handle_blob` maps preview/detail/grid/tags/cues/artwork and
returns 160 zero bytes for user-info; R4 supplies index/ANLZ data. Current
`tests/blobs.rs` compares captured grid/waveform/tag bytes and an extended
cue list. Its supposed plain-cue test is a normalized VBR fixture (13).
`tests/link.rs` additionally exercises socket browse/load and NFS reads.
These prove selected RBX encodings, not every payload or the vendor's
unavailable branch.

[OBS, historical log] `verification/link/kuvo-delivery-20260919.txt`
identifies Windows rekordbox 7.2.11 and booted CDJ-3000 EP122:
`3006→4d02` with 160 bytes, then `2602` and a 13-row delivery render.
It retains a complete nonzero user-info reply. This is a separate historical
Windows capture, not evidence that all 160 bytes are required or that the
macOS callback emits the same account data.

### Unknowns and bounded evidence attempt

Read the complete recorded KUVO exchange, existing blob/socket assertions,
and analysis paths in 13–15. [UNKNOWN] nonzero VBR/segmented-key behavior,
legacy-cue error envelope, absent-data client reaction, and exact required
account fields. Evidence task: pair full vendor/RBX captures for controlled
tracks with absent artwork/analysis, real VBR, memory/hot cues with
comments/colors/loops, and named tags. Account data must remain fixture data;
do not fabricate an identity or claim all-zero is vendor parity.

### Implementation/evidence handoff

Affected: R3 dispatch/envelopes, R4 blobs/catalog and associated fixtures;
update only a demonstrated mismatch. Dependencies: 13–15 explicitly;
18 separates ACK from loaded status; 42 owns account equivalence.
Retain full bytes, cue counts, decoded record assertions, file hashes,
client follow-up and exact source/device build/mode. Do not call omitted
payloads "unavailable" without the corresponding evidenced reply.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test blobs`,
then `RB_LITE_TEST=1 cargo test -p rbl-link --test link`. The paired
capture/client gate remains unpassed.
