# 42. Match account and user-info response behavior

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor 3006 is callback-backed and can return data or empty; RBX always returns 160 zero bytes.

## Task

Trace the vendor callback, field meanings, availability guard, privacy boundary, and client follow-up before implementing any populated response.

## Completion evidence

For available/unavailable cases, full 4d02 envelopes and client follow-ups match the declared scope without fabricating identity fields or leaking host account data.

## Sources and limits

V4 OnUserCmd; R3 USER_INFO; R4 blob helper.

The existing zero reply may satisfy a particular load flow; that does not establish parity for account features.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **availability/envelope/provenance established; populated identity semantics blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `OnUserCmd` (943–1139) accepts 0x3006, allocates and zeros
160 bytes, then calls GetDJID. Contrary to a generic “callback-backed”
description, `GetDJID` (6454–6480) copies exactly 32 bytes from a
non-null stored pointer at +0x648 and returns whether that pointer exists.
Available: 0x4d02 [0x3006, 0, 160, blob], first 32 opaque bytes plus
128 zeros. Unavailable: same reply kind/status with length 0/empty blob.
The serializer retains the transaction and existing requester response route;
the request does not change the active menu or durable storage.

V3 `PSvDBMain::Start` (4901–4951) replaces the stored data, copying
32 bytes from a non-null startup argument only after DBComm start succeeds.
V1 UI startup path (19973–20036) loads a KuvoService nxsFile only after
`isValid`; fewer than 32 loaded bytes means null input. This establishes
data provenance, not field meanings or permission to read the user's live
identity file. R3 `LinkSession::handle_blob` (980–985) unconditionally
produces 160 zeros for USER_INFO; R4 has no user-info availability provider.

### Unknowns and bounded evidence attempt

Traced response → GetDJID → DB startup → UI caller and inspected the
historical `kuvo-delivery-20260919.txt` full 0x4d02 response. That record
is Windows rekordbox 7.2.11 and CDJ-3000 EP122, with nonzero first 32 bytes
and 0x2602 follow-up; it is not same-build macOS unavailable-case proof.
[UNKNOWN] field meanings, nxs validity/identity lifecycle, consent/privacy
policy and client behavior after an empty response. Evidence task: recover
KuvoService validation and client empty-result handling using synthetic
identity fixtures, then capture available/unavailable cases. Do not publish
the captured account bytes as a reusable identity or fabricate a DJ ID.

### Implementation handoff

Affected: R3 Catalog/USER_INFO availability interface, R4 blob provider and
explicit backend ownership/privacy boundary; never auto-read host identity
to fill a packet. Dependencies: 23, 25, 33/35 for any broader account flow.
Fixtures: absent, known synthetic 32-byte value, reset/start replacement and
wrong/truncated data; assert all 160 bytes or exact empty envelope and the
client's next request. Preserve the established CDJ load flow until empty
handling is verified. Smallest check:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session the_kuvo_user_info`.
That existing test covers one full-length response, not unavailable parity.
