# 13. Serve real VBR data for request 2504

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] rekordbox loads track-specific VBR data and distinguishes unavailable data; RBX returns a fixed zero blob labelled CueList.

Direct consequence and boundary: Player seeking/loading effects are unknown. A zero response is not proof that a track is CBR; do not fabricate a VBR map.

## Do this one thing

Trace MstLoadVBR's file/record format and reply envelope, then implement evidence-backed VBR retrieval for 2504 and its unavailable case. Correct misleading protocol names/comments without changing unrelated cue handlers.

## Evidence and code

V4 analysis dispatch; V6 GetVbrInf; R3 CUES; R4 catalog/blobs. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c), [V6](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)
- [crates/rbl-link/src/blobs.rs](../../crates/rbl-link/src/blobs.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Compare complete 4502 replies for a track with nonzero VBR contents, unavailable VBR, and relevant non-VBR inputs. Validate load and seek against matching vendor/device evidence before changing existing compatibility behavior.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **data layout recovered; unavailable-sequence and client validation blocked**.
[Evidence key](investigation.md).

### Contract and current coverage

[OBS] V4 `OnSongAnlzCmd` (3329–3471) routes `2504` with numeric
context and content ID to V6 `GetVbrInf` (1–63). The helper resolves the
content's analysis path, including cloud-relative resolution when applicable,
then calls `MstLoadVBR` (V6 1948–2080). It requires a PMAI file whose
declared size matches the file, PPTH at the first section, then the VBR
section; it also validates following quantize, two wave, and two cue sections.
The checked-in trimmed DAT starts with PQTZ and has no PPTH/PVBR; it cannot
serve as a positive fixture for that loader.

[OBS] The VBR table has 400 big-endian 32-bit words in the file. The loader
converts them to native words, then reads a final big-endian scalar. A
header-plus-four-byte VBR section skips table loading, leaving the
zero-initialized table. `GetVbrInf` returns 1604 bytes: 400 native
little-endian words on the inspected ARM64 build plus the converted scalar.
V4 `RetBinToClient` (3973–4042) defines same-session/same-transaction
`4502 [2504, status, byte_length, blob]`. Success status is 0.
No menu, cue, or database mutation is implied.

R3 calls this `CUES/Analysis::CueList`; R4 `catalog::analysis` (1333–1363)
returns `cue_list_blob()`, always 1604 zero bytes for an existing track.
Those names/comments conflate VBR with cues. The old blob test explicitly
cleared its last bytes as alleged uninitialized cue padding; V6 instead
assigns the final scalar. That normalization is not a valid VBR oracle.

### Unknowns and bounded evidence attempt

Read both complete V6 functions and V4's caller/reply helper; inspected the
trimmed DAT and the captured VBR transactions `1bb`/`1d1` in
`verification/link/dbserver-decoded.txt` (289–290, 411–412).
Their displayed blobs are abbreviated and only show a zero prefix.
On failure, `GetVbrInf` queues an empty status-0 reply and returns 0,
after which its caller queues empty status `0x32`. [UNKNOWN] whether
both reach the client in that order and how it handles them; do not silently
pick one or copy the apparent double reply without a trace. [UNKNOWN]
nonzero-table load/seek behavior and the final scalar's meaning.
Evidence task: retain an untrimmed analyzed VBR file plus its full `4502`,
then trace absent/corrupt/short-table cases and load/seek on the named device.

### Implementation handoff

Affected: R3 constants/catalog analysis enum/session route; R4
`catalog.rs`/`blobs.rs`; `rbl-anlz` section decoding if required.
Dependencies: 01 error boundary and 23 client acceptance. Fixtures:
nonzero 400-word table, compact four-byte body, absent file, corrupt lengths,
unavailable content, exact full envelopes and unchanged menu. Preserve
existing compatibility until the missing failure/client evidence is resolved.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test blobs`,
then `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`.

## Step 2 — implementation (2026-10-05)

Isolated the established naming correction: `2504`/`4502` are now named
`VBR`/`VBR_REPLY`, with `Analysis::Vbr` and
`blobs::vbr_compatibility_blob`. The existing 1604 zero bytes and response
envelope remain unchanged. Comments identify this as retained compatibility
policy; they no longer describe legacy cue records or assert an unavailable
reply hangs a device. Unrelated cue handlers are unchanged.

The old normalized fixture cleared the final VBR scalar as alleged cue
padding and cannot support a vendor-parity assertion. Its misleading
comparison was replaced with a direct regression for the existing placeholder
bytes. A complete encoded session regression checks `4502 [2504,0,1604,blob]`
and unchanged pending menus in both setup forms. These are preservation tests,
not a reference VBR oracle.

Validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink -p rbl-link --lib
--test packets --test blobs` passed (102 tests), as did the focused encoded
session regression and `cargo clippy -p rbl-prolink -p rbl-link
-p rbl-dbserver --all-targets -- -D warnings`. Track-specific retrieval, failure reply
delivery, final-scalar interpretation and device load/seek validation retain
the Step 1 evidence gates; this bounded correction does not complete issue 13.
