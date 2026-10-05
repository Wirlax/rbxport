# 17. Return the evidenced played-state value and context gate

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox returns played value 2 only in context 4; RBX returns boolean 1/0 without that gate.

Direct consequence and boundary: This task concerns the read reply, not a redesign of history storage or on-air state.

## Do this one thing

Match 3b03 reply values and the context condition. Keep the library's internal played representation independent of wire values.

## Evidence and code

V4 OnOtherCmd; R3 TRACK_PLAY_STATE; R4 played adapter. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test played/unplayed tracks in each context, unknown tracks, and complete scalar envelopes. Validate a device's played marker separately if claiming UI parity.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

