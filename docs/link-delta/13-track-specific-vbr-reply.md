# 13. Serve real VBR data for request 2504

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox loads track-specific VBR data and distinguishes unavailable data; RBX returns a fixed zero blob labelled CueList.

Direct consequence and boundary: Player seeking/loading effects are unknown. A zero response is not proof that a track is CBR; do not fabricate a VBR map.

## Do this one thing

Trace MstLoadVBR's file/record format and reply envelope, then implement evidence-backed VBR retrieval for 2504 and its unavailable case. Correct misleading protocol names/comments without changing unrelated cue handlers.

## Evidence and code

V4 analysis dispatch; V6 GetVbrInf; R3 CUES; R4 catalog/blobs. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)
- [crates/rbl-link/src/blobs.rs](../../crates/rbl-link/src/blobs.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Compare complete 4502 replies for a track with nonzero VBR contents, unavailable VBR, and relevant non-VBR inputs. Validate load and seek against matching vendor/device evidence before changing existing compatibility behavior.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

