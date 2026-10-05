# 17. Return the evidenced played-state value and context gate

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox returns played value 2 only in context 4; RBX returns boolean 1/0 without that gate.

Direct consequence and boundary: This task concerns the read reply, not a redesign of history storage or on-air state.

## Do this one thing

Match 3b03 reply values and the context condition. Keep the library's internal played representation independent of wire values.

## Evidence and code

V4 OnOtherCmd; R3 TRACK_PLAY_STATE; R4 played adapter. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test played/unplayed tracks in each context, unknown tracks, and complete scalar envelopes. Validate a device's played marker separately if claiming UI parity.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **wire value/context contract established**.
[Evidence key](investigation.md), including RX3 F1/F2.

### Contract and current coverage

[OBS] V4 `OnOtherCmd` (902–924) handles
`3b03 [context, content_id]`. Only when `(context >> 8) & 0xff == 4`
does it scan the played-ID list; matching membership returns **2**, otherwise
0. For every other context it returns 0 without that lookup.
V4 `Ret4ByteToClient` gives `4000 [3b03, value]` on the originating
transaction/session. Read-only operation: preserve menus, playback state,
history rows and internal played representation.

R3 `TRACK_PLAY_STATE` (1126–1130) converts catalog boolean to 1/0 with
no context guard. RX3 F1 `dbcl_GetTrackPlayState` (`0026633c`,
5520 onward) waits for `4000` and passes the scalar to its output; it
does not prove a value of 1 means the same thing as desktop value 2.
F2's local-device server consults its own status store for slots 2/3,
which is not a reason to remove the desktop gate.

### Unknowns and bounded evidence attempt

Read V4's complete branch and scalar helper, current session behavior and
RX3 request/wait path. Existing decoded captures searched here contain no
`3b03`. [UNKNOWN] exact visible played-marker interpretation and
the vendor list's persistence lifecycle; this read task does not authorize
rewriting history. Evidence task for UI parity: capture a played/unplayed
pair in context 4 and inspect the client marker; leave that claim unverified.
Absent or wrong-typed arguments require safe handling, with vendor malformed
policy still unknown.

### Implementation handoff

Affected: `rbl-dbserver/src/session.rs` and tests; R4 `Catalog::played`
can remain boolean. Dependencies: share byte extraction with 16, but not its
RX3 browse-type behavior. Fixtures: played/unplayed/unknown IDs in 4 and
non-4 contexts, vary menu location independently, complete serialized
envelopes, and unchanged active menu/history. An unknown ID must not become
played merely because an invalid request defaults to an existing ID.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.

## Step 2 — implementation (2026-10-05)

Implemented context-gated played membership: typed `3b03 [context,id]`
consults `Catalog::played` only when context bits 8–15 equal 4, returning
scalar 2 for membership and 0 otherwise. Catalog/history storage remains
boolean and unchanged. Missing/wrong-typed arguments safely return 0 without
defaulting an invalid ID to a played track.

The encoded regression covers played/unplayed/unknown IDs, context 0/1/2/3/
4/5/255 independently of three menu locations, malformed decoded shapes,
complete scalar envelopes and unchanged complete follow-up renders.
`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session played_state_wire`
passed. The visible device marker and played-list persistence lifecycle remain
unverified; this implements the desktop read contract only.
