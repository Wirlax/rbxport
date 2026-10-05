# 21. Prove discovery-to-source-visible reply parity

Priority: P1 evidence gate. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] RBX answers identity/media/property/handshake requests, but the report does not establish complete vendor reply parity or greeting triggers.

Direct consequence and boundary: This is a focused evidence task, not a mandate to remove RX3-specific replies or copy the vendor's entire settings receiver.

## Do this one thing

Trace the vendor reply paths and compare a single core discovery/source-selection sequence on a controlled fixture. Change only demonstrated mismatches in fields, destinations, state guards, or ordering.

## Evidence and code

V5 NormalInterval; V1/V3 lifecycle; R2 status/reply/greeting helpers. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c), [V3](../../../rbxport-private/verification/link/rekordbox-re/rb_bystring.c), [V5](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Record packet-by-packet vendor/RBX comparisons for 10/11, 05/06, 30/31, 16/17 where applicable. Record missing evidence as unknown; do not synthesize a universal ordering.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

