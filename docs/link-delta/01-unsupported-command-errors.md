# 01. Return an error without destroying the active menu

Priority: P0. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] An unknown command produces an empty 4000 menu and replaces RBX's stored menu. rekordbox returns 4003 instead.

Direct consequence and boundary: The client's screen reaction is unknown; the menu replacement and reply difference are source-proven.

## Do this one thing

Implement the evidenced 4003 envelope for unsupported commands, preserving transaction/request kind and the active menu. Keep intentional silent commands and device-specific supported extensions explicit rather than routing them through a blanket error.

## Evidence and code

V4 OnUnknownClientCmd; R3 fallback/handle_menu. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Send an unsupported command between a menu request and its render: check the complete error envelope and that the previous menu still renders. Cover both setup shapes, different menu locations, and supported mobile-query behavior.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

