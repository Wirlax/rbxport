# 14. Answer cue request 2104 with the expected cue envelope

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

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

