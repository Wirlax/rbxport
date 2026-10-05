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

## Step 2 — implementation (2026-10-05)

Implemented the established running-state additions: received primary 9 adds
10, primary 11 adds 12, and exact `OPUS-QUAD` at 9 additionally adds 11/12.
The peer/player stores retain logical identities, including shared IP/MAC,
type/name and raw membership flag bytes from frame offsets `0x25`/`0x35`.
No extra outgoing announcement or Join transition is introduced.

Timer ownership is explicit: directly received keepalives own the existing
six-second clocks; synthetic refreshes neither arm a new timer nor extend an
existing direct timer. A real keepalive to a synthetic slot promotes its own
clock. Source evidence remains V1 `readConfigNotify` at 7533–7553 and V3
`timerFuncAging` at 3598–3643. Expiry applies only the proved 9→10 / 11→12
pairs, with coherent player and greeting cleanup. Synthetic-only players do
not acquire an independent activity-expiry rule. Ordinary direct-member
timeout policy remains unchanged.

The bounded OPUS behavior is literal: expiry/disconnect of 9 removes 9/10;
11/12 remain until their own directly received primary timer, numbered
disconnect, type-7 rediscovery or whole-session teardown removes them.
[UNKNOWN] Any additional vendor path removing all four OPUS slots remains
unresolved. Retention here is not a full OPUS lifecycle-parity claim. General
typed `KeepAlive` APIs still use their existing flag encoding; this validated
membership path preserves the two raw flags without assigning meanings.

Focused synthetic packet/state tests cover exact model matching, pre-running
guards, malformed prefixes/subtype, repeated adds, nondefault flags, member
counts, replacement/shared-IP survivors, independent timer promotion and
refresh, timeout boundaries, paired expiry and the established removal paths.
The ordered assertions cover member-store effects, not the vendor's individual
`notifyMessage` callbacks. RBX exposes player/status snapshots rather than that
IPC notification stream. Callback ordering, delivery and downstream consumer
equivalence remain unimplemented or unverified; this bounded change introduces
no speculative event API and does not satisfy that full-lifecycle requirement.
`RB_LITE_TEST=1 cargo test -p rbl-prolink -p rbl-link --lib --test packets
--test blobs` passed: 61 Link unit tests, 34 packet tests and 7 blob tests.
After the Clippy-driven test assertion adjustments, the focused multideck
rerun and `cargo clippy -p rbl-prolink -p rbl-link -p rbl-dbserver
--all-targets -- -D warnings` passed. No device trace was performed.

### Review remediation — F1/F2/F3 (2026-10-05)

Join now records the first peer type only on a successful Waiting→Discovery
transition. V1 `readConfigNotify` (7256–7271, 7317–7334) uses that history in
Running: a non-type-7 session rejects numbers 9–12, and a type-7 session
rejects 1–4, before primary membership or logical synthesis. This is not
conditioned on interface mode; an actual successful Unknown-mode join also
establishes the classification. Failed wireless original-player attempts
do not establish it. Reset clears it for the next successful session.

Announcement-derived membership now passes V5's shared cached-NetIF sender
guard before dispatch (1354–1367). Synthesis still neither arms nor refreshes timers:
V1 7436–7553, V2 589–608 and V3 3610–3637 establish that a removed slot's
pending deadline can remove its synthetically recreated identity. The peer
table retains that deadline and consumes inactive expirations, while full
session clearing discards all deadlines. Direct keepalives still refresh
their slot's clock.

Regressions use real Join acquisition for ordinary/type-7 sessions, retain
Unknown-mode boundaries, and cover explicit disconnect, rediscovery,
paired removal, recreation and inactive expiry. See [review.md](review.md)
for the remediation validation and re-review. The callback, device and full
OPUS lifetime gaps above remain open; these fixes do not establish full parity.
