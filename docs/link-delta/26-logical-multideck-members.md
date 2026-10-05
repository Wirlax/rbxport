# 26. Synthesize and remove the vendor's paired logical members

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The inspected vendor keepalive branch adds paired IDs for 9/11 and additional OPUS-QUAD entries; RBX waits for separate received IDs.

## Task

Map the exact model/number guards, add the paired members only under those guards, and keep disconnect, address update, ageing, and greeting state coherent across the group.

## Completion evidence

One received qualifying announcement creates the evidenced logical member set; disconnect and ageing remove the evidenced members without affecting unrelated decks. Capture device-visible membership where available.

## Sources and limits

V1 readConfigNotify; V2 readDisconnect; R2 beacon receive and membership.

The report does not establish that every multi-deck device requires synthesis.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **addition rules established; complete group-removal contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V1 `readConfigNotify` (7402–7555) accepts the keepalive in running
states 6/8 after its coexistence checks. A primary device number 9 or 11
causes a member at number + 1 with the same address/MAC/type/flags. Number 9
with model exactly `OPUS-QUAD` additionally creates members 11 and 12.
These are logical identities at a shared IP, not duplicate physical hosts.
Member-add notifications are local; this block sends no extra wire reply.
The aging timer is attached to the received primary member.

[OBS] V2 `readDisconnect` (578–611) and V3 `timerFuncAging` (3592–3653)
remove a primary and its +1 partner for 9/11. They do not visibly remove all
four OPUS members on number 9 removal. V1 `readDiscovery` has a separate
MAC/type-scoped removal path. R2 peer/player maps currently create members
from received numbers and do not apply this synthesized identity rule.

### Unknowns and bounded evidence attempt

Read add, explicit removal, aging and rediscovery together; the announcement
captures contain only 06/00 and no identified multi-deck exercise.
[UNKNOWN] how all four OPUS identities age/leave across every path and
whether another callback removes the second pair. Do not infer one global
four-member deletion from the add path. Evidence task: obtain an OPUS/paired
deck trace with member-state instrumentation for primary/secondary leave,
IP replacement, aging and rediscovery; trace the missing propagation.

### Implementation handoff

Affected: R2 peer identity/member lifecycle, greeting and player notification
consumers; do not evict an IP-scoped resource while another logical member
uses it. Dependencies: 05–08 and 29. Fixtures: number 9 ordinary paired
model, number 11 pair, exact OPUS name vs another model, independent peer
at the same/different IP, updates to existing synthesized slots, and every
removal path. Assert ordered logical notifications and no new outgoing
packet on addition. Smallest check:
`RB_LITE_TEST=1 cargo test -p rbl-prolink`, then
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`.
Only the established add rules are ready for bounded implementation;
full lifecycle parity is not.
