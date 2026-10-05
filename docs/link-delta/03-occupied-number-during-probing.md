# 03. Honor keepalives while selecting a number

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] rekordbox records occupied candidates during probing; RBX's keepalive occupancy path only handles Waiting.

Direct consequence and boundary: Depends on 02 for accurate probe-request behavior; a resulting collision is not itself proven.

## Do this one thing

Record evidenced occupancy from keepalives in the probing state so that an already-announced candidate is not selected through that path.

## Evidence and code

V1 readConfigNotify; R2 Join::hear_keep_alive. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-link/src/join.rs](../../crates/rbl-link/src/join.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Inject a candidate keepalive during probing without a probe reply; assert candidate rejection. Cover unrelated numbers, self-origin traffic, repeated announcements, and the Waiting path.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

