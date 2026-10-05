# 24. Verify the complete basic Link lifecycle end to end

Priority: P1 final gate. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

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

## Step 1 — investigation (2026-10-05)

Disposition: **acceptance contract established; no fresh acceptance run**.
[Evidence key](investigation.md).

### Required evidence and current coverage

This issue is an evidence gate, not a new packet opcode. Its accepted inputs
are the exact fixture, implementation/vendor revisions, device firmware,
host OS, selected network mode and scenario. A passing record must cover
discovery→source-visible→connect→browse→load→play→disconnect→reconnect,
then separate shared-IP logical-deck and collision-recovery cases.
Expected packets/state come from 01–23; unresolved branches cannot be
replaced by a happy-path-only run.

[OBS] `docs/testing-strategy.md` separates mocked/unit, real firmware and
physical-device evidence and names dedicated history/key/cue/artwork gaps.
The private `scripts/e2e-link/README.md` explains that its harness builds
a fixture, boots EP122, drives the live panel, retains logs/screenshots/pytest
results and tears down its own booth. `crates/rbl-link/tests/link.rs`
uses a protocol client and loopback sockets; it does not boot firmware.
The old `interaction-audit-20260920/session-results-final.txt` reports
20 session tests in an older `rekordbox-lite` checkout, not current
end-to-end acceptance at this RBX revision.

### Unknowns and bounded evidence attempt

Read the current strategy, harness contract, socket test and historical
result. [UNKNOWN] current dated post-fix outcomes, complete fixture/binary
hashes for a paired acceptance run, and hardware results. The strategy
describes RX3/AZ firmware models as unavailable; that is a document's
statement, not a fresh emulator-capability check. Evidence task before
execution: inspect actual bootable models and harness prerequisites, run
only those that boot, and retain physical runs separately. No device/emulator
was started for this investigation.

### Evidence handoff

Affected: testing-strategy and dated results/manifests under the private
verification tree; production source only when a traced mismatch warrants it.
Dependencies: 01–23, especially blocked retry/readiness/load contracts;
21–23 must have their actual comparison results. Every case needs timestamp,
exact build/device/mode, fixture hashes, raw captures, state/UI assertions,
pass/fail/blocked result and links. No pass by omission; one CDJ result does
not generalize to RX3, AZ, other firmware, or another OS.

Smallest local prerequisite: `RB_LITE_TEST=1 cargo test -p rbl-link --test link`.
For an authorized later firmware run, use the documented
`../rbxport-private/scripts/e2e-link/run.sh` from its checkout with fixture
isolation; record a booted/ticking panel and actual results. Physical
playback and cable/reconnect proof remain separate. Completion of this
investigation does not mark basic parity achieved.
