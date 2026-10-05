# 44. Resolve file read and directory edge behavior

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The report identifies 64,512 versus 8192 read caps, zero-count status, attribute and directory freshness, count budgets, and symlink lookup differences.

## Task

For each difference, determine client reachability and expected wire result, then implement behavior required by the full claim while preserving path boundaries and read-only NFS semantics.

## Completion evidence

Captured or firmware-driven tests cover large/zero/EOF reads, repeated attributes, directory mutation/pagination/cookies, symlinks, and malformed handles, with exact statuses and data.

## Sources and limits

V7 NFS READ/READDIR/attributes/lookup; R5 NFS/VFS.

Do not enlarge exports or follow escaping symlinks to copy an internal implementation detail.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **read/directory differences established; parity versus compatibility policy unresolved**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V7 READ (401–445) consumes 32-byte handle + BE32 offset/count/
obsolete total-count, caps count at 0xfc00 (64512), and replies with
status/attributes/actual data length/padded bytes. `_tkfFSReadFile`
(2266–2329) opens the current path on each request; any zero-byte fread,
including requested count zero or EOF, yields IO (5). Invalid handle yields
STALE (70); positive short reads succeed. Current R5 caps at 8192, already
returns IO for positive-count EOF, but returns successful empty data for
zero count. Its open/read-ahead cache can retain earlier file data.

V7 GETATTR/`_tkfGetFileAttributes` (1778 onward, 1970 onward) restat
the current path; R5 VFS `attributes` caches metadata in OnceLock.
V7 READDIR (693–728, 2401–2569) refreshes host entries at cookie zero,
uses file-ID cookies and returns IO for unknown continuation cookies.
Entry budget is caller count for entries plus terminator/EOF, excluding the
28-byte RPC/status prefix; each entry needs padded name + 16 bytes and
8 bytes reserved for termination. R5 caches directory listings, clamps
count to 512..8192 and includes already-built header bytes in its limit.
These affect more than just maximum packet size.

V7 LOOKUP uses stat/path resolution and can follow symlinks;
READLINK itself returns ACCES. R5 intentionally refuses symlink entries
and confines traversal to allowed roots. This investigation does not
authorize exposing host paths or following escaping links.

### Unknowns and bounded evidence attempt

Read these handlers, VFS and cache code. Link-export capture inventory
shows READ counts 4096 (1), 12288 (1), 16384 (13), 32768 (679):
large requests are reachable; no zero-count request was found. It does not
prove that a short successful response is harmful. R5's current MAX_READ
comment records a historical physical-device retransmission reason for
8192; that claim was not rerun here.
[UNKNOWN] exact client impact of short reads, refresh/cookie ordering during
mutation, small-budget edge cases and safe in-root symlink compatibility.
Evidence task: paired fixture capture with immutable/replaced/growing files,
zero/EOF reads, directory restart/continuation and controlled symlinks;
compare reassembled replies, retry counts and client progress. Do not
increase datagrams merely to remove a numeric difference.

### Implementation handoff

Affected: R5 VFS metadata/directory invalidation, READ cache and XDR budgets;
R4 export registration must remain confined. Dependencies: 19, 29, 43, 45.
Use temporary files with exact hashes, two clients, non-ASCII names, stable/
stale handles, count boundaries and rename/delete between calls. Assert data,
attributes, cookies, status, cache invalidation and full reply sizes.
Smallest check: `RB_LITE_TEST=1 cargo test -p rbl-nfs`.
Any retained 8192/symlink restriction must be an explicit scoped difference,
not silently described as byte-for-byte full parity.

## Step 2 — bounded zero-byte READ implementation (2026-10-05)

[OBS] Re-read V7 `filsine.c` `_tkfNFSProcedureRead` (401–445,
`0x343c`) and `_tkfFSReadFile` (2266–2329, `0x5464`). The helper
returns IO (5) for any zero-byte `fread`, including a requested count of
zero, while positive short reads succeed. R5 now returns status-only IO
whenever the selected file read produces no bytes; the previous
positive-count exception has been removed. Invalid handles still return
STALE (70), and malformed required handle/offset/count fields still return
the existing accepted RPC GARBAGE_ARGS envelope.

Five isolated temporary-file fixtures in `crates/rbl-nfs/tests/protocol.rs`
compare complete RPC replies, including XID, accepted status and null
verifier. Coverage includes zero count on nonempty/empty files, positive
count on an empty file, exact/past EOF, stale handles with zero/positive
count, every truncation of the required handle/offset/count fields, and a
positive short read after a zero-count request through the same cache.
The short-read fixture checks all seventeen attribute words, actual data
length, bytes and XDR padding; file contents remain unchanged. The
malformed cases preserve RBX's existing bounds checks and do not establish
vendor malformed-input parity or obsolete total-count parsing.

Validation: the focused `RB_LITE_TEST=1 cargo test -p rbl-nfs --test protocol
read_edge` passed **5 tests** (zero-count regressions failed before the
condition change); `RB_LITE_TEST=1 cargo test -p rbl-nfs` passed **62 tests**
(61 protocol fixtures and 1 read-ahead unit test).
`cargo clippy -p rbl-nfs --all-targets -- -D warnings` passed.

This is static-source and local RPC-fixture evidence. No new capture,
booted-firmware or physical-device validation was performed. The 8192-byte
cap, read-ahead/open-file cache, export confinement and symlink refusal
remain explicit scoped differences. Metadata/directory refresh, cookies,
budgets, cache lifetime and broader issue 44 acceptance remain blocked;
this change makes no full NFS parity claim.
