# 24. Verify the complete basic Link lifecycle end to end

Priority: P1 final gate. Status: planned. Source: [comparison report](../../link-delta.md).

## Why this task exists

[OBS] The report is static evidence plus older captures/tests, not a fresh end-to-end parity demonstration.

Direct consequence and boundary: No implementation is complete merely because unit tests pass. This task defines the basic-parity claim, not full rekordbox feature parity.

## Do this one thing

Run and retain a basic discovery, connect, browse, load, play, disconnect, reconnect acceptance sequence after the preceding changes. Include multi-deck identity and collision recovery as separate cases.

## Evidence and code

Source report evidence boundary; tasks 01–23; current docs/testing-strategy.md.

Implementation locations:

- [docs/testing-strategy.md](../../docs/testing-strategy.md)
- [crates/rbl-link/tests](../../crates/rbl-link/tests)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Publish scenario results, fixture/binary hashes, packet traces, implementation revision, and exact tested devices/builds. Separate mocked tests, socket integration, booted vendor firmware, and physical hardware. Do not generalize one device result to others.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

