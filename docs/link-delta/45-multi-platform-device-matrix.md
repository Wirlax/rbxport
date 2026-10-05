# 45. Validate each claimed OS, device, firmware, and network mode

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

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

