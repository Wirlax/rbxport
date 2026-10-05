# 39. Match remaining database scalar, notice, and silent replies

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The report identifies reply differences for rating, history removal context, 3203/3503 notices, 3402 menu side effects, and other unsupported branches.

## Task

Audit the complete database dispatch/reply matrix, then correct one command's envelope, silence, result value, and menu-state effect at a time. Preserve device-specific additions with verified reply shapes.

## Completion evidence

Packet fixtures cover success, failure, foreign context, unsupported command, and follow-up render for each row; no false success or unsolicited reply remains in the claimed surface.

## Sources and limits

V4 OnDbModCmd/OnHistoryCmd/OnOtherCmd/OnUnknownClientCmd; R3 session.

Do not translate all backend booleans with one common convention; vendor commands use different scalar meanings.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **listed reply distinctions established; complete callback matrix still blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V4 `Ret4ByteToClient` (1686–1745) sends 0x4000
[original request kind, scalar] with the original transaction to the
requester's existing response route; it does not select/change a browse menu.
The inspected dispatcher distinguishes these cases:

| Request | Vendor behavior in the inspected handler |
| --- | --- |
| 0x2107 / 0x2507 | Reply with backend result unchanged; 1 triggers rating/BPM local update (425–507). |
| 0x3001 / 0x3101 / 0x3201 | With context low byte 1, history/on-air processing, no wire reply here; foreign context also silent (507–622). |
| 0x3401 | Context low byte 1: backend result reply and history-update event; foreign context: no reply or write. |
| 0x3203 | Local DeliverError notice, no wire reply (802–943). |
| 0x3503 | Mark handled without wire reply. |
| 0x3402 | Scalar 0; no list query/cache replacement in OnPrepareCmd (622–731). |
| 0x3903 | Scalar 0. |
| 0x3c03 | Scalar 1 iff argument 1 masked by 0xfe is zero; this is not a proven full-value range check. |
| 0x3003 / 0x3603 / 0x3a03 / 0x3d03 / 0x3008 | Distinct color/hierarchy/key/BPM consumers, not one boolean convention (802–943, 1139–1170). |
| Unsupported kind | 0x4003 [original kind], retaining active menu; issue 01. |

[OBS] R3 `Session::handle` currently returns inverted success for rating,
answers foreign-context 0x3401, and sends unhandled notices/scalars through
the generic 0x4003 fallback. That fallback was already fixed at this
baseline; the earlier “empty menu for every unsupported command” is stale.
Browse-type and played-state fixes have separate contracts in 16–17.

### Unknowns and bounded evidence attempt

Read the full cited dispatchers, scalar serializer and R3 dispatch, and
checked the historical request list. [UNKNOWN] all backend-specific
negative codes, lock/open failure routing, mobile response-route variants
and device reactions to each silent notice. Evidence task: complete issue
25's per-command matrix with exact serialized exchanges and post-command
renders for both setup forms. Internal return values and DeliverError
callbacks are not automatically packets.

### Implementation handoff

Affected: R3 dispatch/scalar handling, R4 typed edit results, callback
consumers only where their delivery is proven. Dependencies: 01, 16–17,
25, 37–38, 40–42. Fixtures must isolate each row: successful, failed,
malformed and foreign context; assert empty output where silent and
unchanged active menu via subsequent render. Smallest check:
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.
Implement one established row at a time, not a generic bool→status mapper.

## Step 2 — implementation (2026-10-05)

Narrow implementation of established rows: `3203/3503` are silent;
`3402/3903` return `4000 [kind,0]` without menu replacement;
foreign-context `3401` neither replies nor calls the history edit.
The own-context history branch and its existing backend-result adapter were
otherwise preserved.

Encoded tests cover complete scalar responses, intentional silence, foreign
history context and complete unchanged follow-up renders for both setup modes.
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session` passed.
The full dispatcher matrix and callback-specific negative codes,
rating/BPM result adapters and notification consumers remain blocked; these
isolated rows do not claim whole-issue parity.

An additional established row now handles `3c03 [context,value]`. V4
`OnOtherCmd` case 0xc (917–920) returns scalar 1 exactly when
`(value & 0xfe) == 0`; high bits are ignored, so 256/257/0x10000 return 1.
There is no context gate in this case, including typed foreign contexts.
The operation returns the same-transaction `4000 [3c03,scalar]` without
replacing either pending menu or mutating settings/storage. Missing or
wrong-typed context/value is refused with `4003 [3c03]` as an RBX safety
policy; that malformed-request policy is not claimed as vendor parity.

The complete encoded regression covers 0/1/2/255/256/257/0x10000, ordinary
and foreign contexts, both setup forms, malformed decoded shapes and both
root/track menu renders unchanged byte for byte. The focused test passed;
`RB_LITE_TEST=1 cargo test -p rbl-dbserver` passed **79 tests**, and
`cargo clippy -p rbl-dbserver --all-targets -- -D warnings` passed.
This remains source/fixture evidence, without a fresh vendor/device trace.
