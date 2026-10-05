# 38. Match filter conditions and session persistence

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor filter dispatcher has MyTag conditions and a context-associated setting manager with save-file support; RBX filter state is session-local and lacks MyTag operations.

## Task

Trace all condition encodings, owner/context lookup, save/reload triggers, and failure replies, then implement the evidenced scope.

## Completion evidence

A filter configured in one session returns the expected matches and persists or resets across reconnect exactly when the vendor does; MyTag conditions and 0x32 failure cases are covered.

## Sources and limits

V4 OnFilterCmd/filter helpers; V6 saveFile; R3 filter/session.

The existence of saveFile does not establish its call timing; recover it before selecting persistence behavior.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **wire dispatch/guards partly established; ownership and persistence blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `OnFilterCmd` (731–802) invalidates the requester list cache
before handling 0x3007–0x3407. Setters return 0x4000 [kind, 0] only when
their helper returns 1; otherwise scalar 0x32. 0x3107 uses a distinct
0x4004 binary-plus reply with record count. Helpers (5643–6143) share a
lazily created FilterSettingManager; enabled snapshots are indexed by the
context's high requester byte, not solely by TCP session.

0x3207 requires argument 2 == 0, length argument 3 > 3, and an LE16
count in blob bytes 2–3 matching length 4 + 4*count; the blob begins
flags/operator, followed by LE32 values. 0x3307 requires argument 1 == 0
and operator argument 4 exactly 16 or 17, then sets category argument 2
and flags argument 3. 0x3407 uses argument 2 category, argument 3 item,
argument 4 add/delete; item 0xffffffff is allowed only for delete-all.
Successful changes refresh an existing enabled snapshot for requesters 1–12.
The property getter excludes category 0x19, includes available categories/
signs, and emits 0x32 with empty data/count when none are available.

[OBS] V6 `saveFile` (1236–1634) and `loadFile` (5719–5980) use FMAI/
FCND records, preserve/rewrite file records, restore defaults for missing
standard categories, and reload after a failed replacement. MyTag category
0x19 receives special handling, not straightforward serialization with the
four scalar properties. R3 TrackFilter holds only properties 6/7/12/15,
is session-local and has neither MyTag command; current setter failures use
a different scalar convention.

### Unknowns and bounded evidence attempt

Read all five handlers, save/load and current filter; historical
`filter-properties.txt` contains a 148-byte/four-record 0x3107 response
and unsupported 0x3607. [UNKNOWN] manager constructor path/owner, setter
save timing, MyTag default/validation semantics and reconnect/restart
lifetime. Only load-time default repair visibly calls saveFile in these
exports; saveFile's existence does not prove save-on-every-set.
Evidence task: recover constructor/setCondition/MyTag methods and trace
set→get→new-session→restart, including failed storage.

### Implementation handoff

Affected: R3 filter/session and typed shared owner; R4 index-backed MyTag
matching; backend settings persistence after ownership is proven.
Dependencies: 25, 36, 39. Fixtures: exact FCND blobs/order/counts, invalid
reserved fields/operator/length, two requesters, disabled snapshots,
delete-all, unavailable category, reconnect and write failure in temporary
storage. Smallest check: `RB_LITE_TEST=1 cargo test -p rbl-dbserver filter`.
Do not pick session or disk lifetime by convenience.

## Step 2 — implementation (2026-10-05)

Narrow implementation: existing `3207` property setters now require
typed context/property/reserved/length/blob fields, reserved argument 2 equal
to zero and length >3 matching the blob. Established setter failure emits
scalar 0x32; success emits 0. The existing parser and session ownership are
otherwise preserved.

Encoded tests cover wrong-typed context/reserved/blob, absent arguments,
unknown property, reserved nonzero and inconsistent lengths. Failures preserve
the complete property-get response and pending menu render; valid and currently
accepted typed foreign-context requests retain their existing path.
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session` passed.
MyTag operations, requester ownership, cache invalidation and save/reconnect
lifetime remain blocked; no disk persistence policy was selected.
