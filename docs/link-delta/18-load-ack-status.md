# 18. Distinguish load-command response statuses

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox parses MusicDD response information; RBX logs every 1a as accepted.

Direct consequence and boundary: Status meanings must be recovered before implementation; callback forwarding alone does not prove enum semantics.

## Do this one thing

Parse and validate the evidenced status fields; report acceptance only for an established acceptance value. Unknown statuses must remain explicit, not treated as success.

## Evidence and code

V5 NormalInterval/MusicDDResInfo::setData; R2 LOAD_TRACK_ACK branch. Primary artifacts: [V5](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Feed known accepted/rejected responses, unknown values, and truncated packets; check reported state/logs. Verify a later status packet—not ACK alone—establishes actual load completion.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

