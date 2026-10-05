# 25. Inventory every Link request and reply before a full parity claim

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The existing audit lists inspected handlers and gaps but does not prove an exhaustive request/response inventory across connection modes, devices, and follow-up flows.

## Task

Build a versioned coverage matrix from vendor dispatchers, callbacks, captured packets, and client firmware. For each operation record request shape, state guard, response or silence, mutation, follow-up, RBX route, and evidence level.

## Completion evidence

A reviewer can trace every reachable vendor operation to an RBX behavior or an explicit, scoped exclusion, with no unexplained handler or callback left in the claimed surface.

## Sources and limits

V1–V7 vendor dispatchers; R1–R5 RBX modules; link-delta.md evidence appendix.

This is an evidence gate. Handler names and absent captures do not prove a command is unused.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **inventory specification established; exhaustive coverage unproven**.
[Evidence key](investigation.md).

### Contract and inspected surface

This is an evidence artifact, not a packet to acknowledge. Its input is a
versioned tuple of vendor build, host OS, device/firmware, network mode and
session setup. Each matrix row must identify transport/port, direction,
opcode/subtype, exact fields/lengths, state guard, response or intentional
silence, destination, in-memory/storage mutations, client follow-up, RBX
route, source address/capture frame and evidence level. Replies and local
message-queue IDs must not be catalogued as new wire requests.

[OBS] V1 `SysMgrMainComponent::frameRead` (6670–6830), V5
`NormalInterval::messageReceived`, `ShortInterval::messageReceived`,
`Monitor::messageReceived`, TCP/DeviceConnect receivers, V4
`OnListClientCmd` and individual command handlers, and V7
`_tkfRpcIncomingData` and its service dispatchers cover different surfaces.
V3 `InnerLinkAPI::linkProc`
adds callback/state dependencies after packet parsing. R2 announcement/status
loops, R3 `LinkSession::handle`, and R5 `Server::handle_from` are the corresponding
current RBX entry points; there is no single dispatch table proving coverage.

### Unknowns and bounded evidence attempt

Read these dispatchers and the available capture inventories in the shared
record. [UNKNOWN] complete reachability across mobile, HID, wireless, other
OS builds and callback-generated follow-ups. The five captures cover narrow
scenarios, not every branch; several callback definitions are absent from
the supplied exports. An absent packet cannot justify exclusion. Evidence
task: finish the row-by-row closure from dispatch to callback/emitter, obtain
missing function exports and scenario traces, and have exclusions explicitly
bound the claim. Issues 26–44 supply the already identified open families.

### Evidence handoff

Affected: a versioned coverage artifact under this directory/private
verification tree; R1–R5 only for subsequently established missing behavior.
Dependencies: 01–24 contracts plus 26–44; 45 validates and 46 reviews the
result. Fixtures must span both setup forms, menu locations, malformed and
foreign-context inputs, operation success/failure and reconnect, with full
bytes and state before/after. First useful local check:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`; it is only an
inventory prerequisite, not an exhaustiveness test. This investigation
does not declare the full matrix complete.
