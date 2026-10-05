# 05. Disconnect the correct logical deck and its peer entry

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] RBX removes all players at the sender IP and leaves a peer entry; rekordbox removes the payload-number member with specific paired-ID rules.

Direct consequence and boundary: Do not replace IP-wide removal with unconditional single-deck removal: vendor paired-ID behavior must be retained.

## Do this one thing

Resolve disconnects by the evidenced logical identity, maintain coherent player/peer/greeting state, and implement only verified paired-ID removal rules.

## Evidence and code

V2 readDisconnect; R2 hear_announce/remove_players_at. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Create two logical decks sharing one IP, disconnect one, and assert exactly the evidenced remaining members and cleared greeting/peer state. Cover 07/08 and special IDs separately.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

