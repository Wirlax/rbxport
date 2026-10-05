# 15. Answer key-information request 2a04

Priority: P1. Status: planned. Source: [comparison report](../../link-delta.md).

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

