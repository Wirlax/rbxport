# 36. Implement play-count and MyTag browse families

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor list dispatcher has play-count family 0e and MyTag family 15; RBX has no explicit menus for them.

## Task

Trace hierarchy, filters, sort, item layout, pagination, context, empty state, and follow-up requests; implement each family with the existing indexed query boundary.

## Completion evidence

A client can navigate both families to tracks, with vendor-equivalent envelopes/order for representative libraries and correct empty/unknown-item behavior.

## Sources and limits

V4 OnListClientCmd and called helpers; R3 menu dispatch; R4 catalog/index.

These families are deferred from basic parity; handler presence alone is not a complete browse implementation.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **opcodes/argument routing established; complete menu contract blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `OnPlayCntListCmd` (2033–2085) accepts 0x100e (root)
and 0x110e (tracks); it passes context and DB index to backend vtable
slots +0x138/+0x140. The tracks branch additionally passes the low byte
of argument 2 and the signed low byte of argument 1. Do not assume the
latter is a full 32-bit count. V4 `OnMyTagListCmd` (2921–2969)
accepts 0x1015 and 0x1315, not every opcode ending in 0x15.
0x1015 passes context, argument 2 and boolean argument 3 to +0x1f0;
0x1315 passes context and argument 2 to +0x210. Both use
`SharedDBScopedLockAndOpen`; unavailable callback/unknown kind returns
internal -2, not an automatically successful empty menu.

V4 `OnListClientCmd` (1–132) caches an eligible result and sends
`Ret4ByteToClient`: transaction-preserving 0x4000 [request kind, result]
for the handled result path. The eventual render rows are produced elsewhere.
R3 has neither family in its menu dispatcher. R4/catalog and
`rbl-index/src/lib.rs`, `load.rs`, `filter.rs` already contain play
counts/MyTag categories and membership; missing wire menus do not mean the
underlying data is absent.

### Unknowns and bounded evidence attempt

Read both handlers, their list wrapper and current query/index types;
searched supplied exports for the named implementations and the historical
interaction request list for these four opcodes (no hits).
[UNKNOWN] bucket boundaries, tag hierarchy/meaning of arguments,
sorting/ties, row flags/layout, pagination and missing-ID behavior.
Evidence task: resolve the four backend vtable targets and capture full
root→selection→tracks→render sequences, with empty and unknown selections.
Do not invent buckets or equate MyTag with the separate Tag List.

### Implementation handoff

Affected: R3 constants/query/session/item codec, R4 catalog adapter,
`crates/rbl-index` query/filter boundary and focused tests. Dependencies:
22, 25, 38 for filter interaction. Fixtures need repeated/zero/large play
counts, categories/tags with empty and overlapping membership, non-ASCII
names, sort ties and both menu locations. Assert exact header/items/footer,
result counts, stable order and follow-up load IDs; no writes.
Smallest checks: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`
and `RB_LITE_TEST=1 cargo test -p rbl-index`. Only dispatcher routing is
established; complete browse implementation remains blocked.
