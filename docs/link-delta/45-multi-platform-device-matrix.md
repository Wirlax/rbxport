# 45. Validate each claimed OS, device, firmware, and network mode

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The static comparison uses one inspected macOS rekordbox build, and basic acceptance can cover only recorded devices and modes.

## Task

Define the exact supported matrix. For each cell, run connection, browse, load, writes/settings as applicable, fault/reconnect, and packet comparison against the matching reference build.

## Completion evidence

Every advertised cell has dated traces, binary/app/device versions, pass/fail results, and unresolved exceptions. Unsupported cells are named rather than generalized from another device.

## Sources and limits

link-delta.md evidence boundary; docs/testing-strategy.md; task 24 acceptance.

Booted CDJ-3000 evidence does not validate AZ/RX3 or another rekordbox release.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **matrix acceptance contract established; no new matrix cells validated**.
[Evidence key](investigation.md).

### Required inputs, outputs and guards

This evidence gate has no new packet shape. Each cell must explicitly bind
RBX revision/build, reference rekordbox executable/library hashes, host OS/
architecture, physical device and firmware, transport/network mode, interface,
library fixture and optional account/settings capability. A scenario applies
only when those inputs and prerequisites are present; “same opcode” is not
a substitute for another device/build.

For each advertised cell, retain discovery/connect/browse/load/play,
applicable write/settings/account features, multi-deck identity,
fault/disconnect/reconnect and the complete request/reply/state comparison.
Expected replies, silence, destinations and storage effects are the
corresponding issue contracts. An unresolved reachable operation is a
blocked/failing cell or explicit exclusion, not a passed cell.
Result artifacts must include timestamps, source/fixture hashes, raw packet
captures, decoded exchanges, state/UI observations and reproducible commands.

### Current evidence and bounded attempt

[OBS] Re-read `docs/testing-strategy.md`, issue 24, source provenance and
the five retained captures. The strategy distinguishes mocked/session/socket,
booted firmware and physical evidence; its baseline names CDJ-3000 3.20,
RX3 1.20 (an activation capture 1.19), and AZ 1.30. Its claimed bootable
automation is CDJ only; RX3/AZ are documented unavailable there. That is not
a fresh runtime check. The static host export is macOS 7.2.11.0342;
the separate KUVO trace is Windows 7.2.11/EP122. Neither validates a full
macOS+Windows × three-device × wired/wireless matrix.

[UNKNOWN] the intended advertised matrix and fresh post-fix results for
its cells. Evidence task before running: enumerate actual target cells and
available firmware/hardware/network modes, verify harness prerequisites,
then execute dated fixture scenarios only on real bootable/available targets.
No device, installed library or live account was changed in this pass.

### Evidence handoff

Affected: versioned matrix/results under docs/private verification and
`docs/testing-strategy.md`; source only for separately traced defects.
Dependencies: 24–44 and issue 25's reachability inventory. Fixtures include
foreign/malformed input, storage failure and network faults, not just happy
paths. Local prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-link --test link`.
A later authorized firmware run uses the documented private
`scripts/e2e-link/run.sh` with a verified booted/ticking panel.
Physical controls/audio and missing models require named physical checks.
Investigation complete does not mean matrix acceptance complete.
